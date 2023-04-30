use chrono::prelude::*;
use log::warn;
use my_mensa_lib::{DayMenu, LinkedHashMap, UserProfile};
use std::sync::atomic::Ordering::Relaxed;
use std::{future::IntoFuture, sync::atomic::AtomicBool};
use teloxide::{
    dispatching::{
        dialogue::{self, serializer::Json, ErasedStorage, InMemStorage, SqliteStorage, Storage},
        UpdateHandler,
    },
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId},
    utils::command::BotCommands,
};
use tokio::join;

static STAGING: AtomicBool = AtomicBool::new(true);

type MyDialogue = Dialogue<State, ErasedStorage<State>>;
type MyStorage = std::sync::Arc<ErasedStorage<State>>;
type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().unwrap();
    pretty_env_logger::init();
    log::info!("Starting mensa bot...");

    match std::env::var("PRODUCTION") {
        Ok(v) if v == "1" => {
            STAGING.store(false, Relaxed);
            log::warn!("Running in production mode!");
        }
        _ => {
            log::info!("Runninng in staging mode.");
        }
    };

    let bot = Bot::from_env();

    let storage: MyStorage = if std::env::var("PERSISTENCE_SQLITE").is_ok() {
        SqliteStorage::open("db.sqlite", Json)
            .await
            .unwrap()
            .erase()
    } else {
        InMemStorage::new().erase()
    };

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![storage])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "Display this help text")]
    Help,
    #[command(description = "Restart the welcome dialog")]
    Start,
    #[command(description = "Show menu for the next days")]
    Menu,
    #[command(description = "/order [date]: Display order form")]
    Order,
}

#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
enum State {
    #[default]
    WaitingForFirstName,
    WaitingForLastName {
        first_name: String,
    },
    ReceiveEmail {
        first_name: String,
        last_name: String,
    },
    Idle {
        user: UserProfile,
    },
    WaitingForOrderSelection {
        user: UserProfile,
        iso_date: String,
        order_select_message: MessageId,
    },
    WaitingForSlotSelection {
        user: UserProfile,
        iso_date: String,
        order_md5: String,
        slot_select_message: MessageId,
    },
}

fn make_timeslot_buttons(slots: &LinkedHashMap<String, i32>) -> InlineKeyboardMarkup {
    let keyboard: Vec<Vec<InlineKeyboardButton>> = slots
        .iter()
        .filter(|(_, &nr_free)| nr_free > 0)
        .map(|(time, nr_free)| {
            vec![InlineKeyboardButton::callback(
                format!("{} ({} free)", time, nr_free),
                time,
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(keyboard)
}

fn make_menu_buttons(menu: &DayMenu) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];

    for meal in menu.meals.iter() {
        let row = vec![InlineKeyboardButton::callback(
            meal.combined_name.clone(),
            meal.md5.clone(),
        )];

        keyboard.push(row);
    }

    InlineKeyboardMarkup::new(keyboard)
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let set_cm_f = bot.delete_my_commands().into_future();
    let sm_f = bot
        .send_message(msg.chat.id, "Hello! Please enter your first name.")
        .into_future();
    let (set_cm_r, sm_r) = join!(set_cm_f, sm_f);
    set_cm_r?;
    sm_r?;

    dialogue.update(State::WaitingForFirstName).await?;
    Ok(())
}

async fn receive_first_name(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        format!(
            "Thanks, {}! Please enter your last name.",
            msg.text().unwrap()
        ),
    )
    .await?;
    dialogue
        .update(State::WaitingForLastName {
            first_name: msg.text().unwrap().to_owned(),
        })
        .await?;
    Ok(())
}

async fn receive_last_name(
    bot: Bot,
    dialogue: MyDialogue,
    first_name: String,
    msg: Message,
) -> HandlerResult {
    let last_name = msg.text().unwrap();
    bot.send_message(
        msg.chat.id,
        format!(
            "Thank you, {} {}! Please enter your email address.",
            first_name, last_name
        ),
    )
    .await?;
    dialogue
        .update(State::ReceiveEmail {
            first_name,
            last_name: last_name.to_owned(),
        })
        .await?;
    Ok(())
}

async fn receive_email(
    bot: Bot,
    dialogue: MyDialogue,
    (first_name, last_name): (String, String),
    msg: Message,
) -> HandlerResult {
    let email = msg.text().unwrap();
    bot.send_message(
        msg.chat.id,
        format!(
            "This completes the setup! If the following is incorrect, please restart the setup using /start.\nFirst name: \"{}\"\nLast name: \"{}\"\nEmail: \"{}\"",
            first_name, last_name, email
        ),
    )
    .await?;

    let state_update_f = dialogue
        .update(State::Idle {
            user: UserProfile::new(first_name, last_name, email.to_owned()),
        })
        .into_future();

    let set_cm_f = bot.set_my_commands(Command::bot_commands()).into_future();

    let (su_r, cm_r) = join!(state_update_f, set_cm_f);
    su_r?;
    cm_r?;

    Ok(())
}

async fn invalid_state(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    warn!(
        "Invalid state callback. message: {:?}, dialogue: {:?}",
        msg.text(),
        dialogue.get().await
    );
    bot.send_message(
        msg.chat.id,
        "Sorry, an error has ocurred in the bot! Restarting dialogue.",
    )
    .await?;
    start(bot, dialogue, msg).await?;

    Ok(())
}

fn any_slots_available(slots: &LinkedHashMap<String, i32>) -> bool {
    for (_, num_free) in slots {
        if *num_free > 0 {
            return true;
        }
    }
    false
}

async fn meal_select_callback(
    bot: Bot,
    dialogue: MyDialogue,
    (user, iso_date, order_select_message): (UserProfile, String, MessageId),
    q: CallbackQuery,
) -> HandlerResult {
    let slots = my_mensa_lib::get_free_slots(2, &user.email, &iso_date).await?;

    if !any_slots_available(&slots) {
        dialogue.update(State::Idle { user }).await?;
        bot.send_message(dialogue.chat_id(), "No free slots are available!")
            .await?;
        return Ok(());
    }

    let keyboard = make_timeslot_buttons(&slots);

    let delete_select_f = bot.delete_message(dialogue.chat_id(), order_select_message);

    let send_slot_select_f = bot
        .send_message(dialogue.chat_id(), "Select Time Slot")
        .reply_markup(keyboard);

    let (delete_res, send_res) = join!(
        delete_select_f.into_future(),
        send_slot_select_f.into_future()
    );
    delete_res?;
    let slot_select_msg = send_res?;

    dialogue
        .update(State::WaitingForSlotSelection {
            user,
            iso_date,
            order_md5: q.data.unwrap(),
            slot_select_message: slot_select_msg.id,
        })
        .await?;

    Ok(())
}

async fn slot_select_order_callback(
    bot: Bot,
    dialogue: MyDialogue,
    (user, iso_date, order_md5, slot_select_message): (UserProfile, String, String, MessageId),
    q: CallbackQuery,
) -> HandlerResult {
    bot.send_message(
        dialogue.chat_id(),
        format!("Ordering \"{:?}\" for {:?}", order_md5, user),
    )
    .await?;

    let selected_slot = q.data.unwrap();

    let mensa_id = 2;

    if STAGING.load(Relaxed) {
        log::info!(
            "STAGING: Not actually ordering anything. Would order: {:?}, {:?}, {:?}, {:?}, {:?}",
            iso_date,
            order_md5,
            mensa_id,
            user,
            selected_slot
        );
    } else {
        my_mensa_lib::order(
            iso_date.as_str(),
            &order_md5,
            mensa_id,
            &user,
            &selected_slot,
        )
        .await?;
    }

    let delete_f = bot
        .delete_message(dialogue.chat_id(), slot_select_message)
        .into_future();

    let res_msg_f = bot
        .send_message(dialogue.chat_id(), "Ordered!")
        .into_future();

    let state_update_f = dialogue.update(State::Idle { user });

    let (delete_res, res_send_res, state_update_res) = join!(delete_f, res_msg_f, state_update_f);
    delete_res?;
    res_send_res?;
    state_update_res?;

    Ok(())
}

fn select_date<'a>(dates: Vec<&'a str>, explicit_date: Option<&'a str>) -> Option<&'a str> {
    log::debug!(
        "Selecting date from {:?}, explicit: {:?}",
        dates,
        explicit_date
    );
    if let Some(ex) = explicit_date {
        if dates.contains(&ex) {
            log::debug!("Explicit date found, returning that.");
            return Some(ex);
        } else {
            return None;
        }
    }

    let now = Local::now();
    log::debug!("Time now is {:?}", now);

    let dates: Vec<_> = dates
        .iter()
        // Parse dates, assume 12:00
        .filter_map(|&s| {
            Local
                .from_local_datetime(&NaiveDateTime::new(
                    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap(),
                    NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
                ))
                .single()
                .map(|t| (s, t))
        })
        .collect();
    log::debug!("Dates: {:?}", dates);
    // Filter dates in the past (current date, if later than 12:00)
    let future_dates: Vec<(&str, DateTime<Local>)> =
        dates.into_iter().filter(|(_s, t)| t > &now).collect();
    log::debug!("Future dates: {:?}", future_dates);
    // Earliest time
    let min: Option<(&str, DateTime<Local>)> = future_dates.into_iter().min_by_key(|&(_s, t)| t);

    log::debug!("Min: {:?}", min);

    // Back to string
    min.map(|(s, _t)| s)
}

async fn present_order(
    bot: Bot,
    dialogue: MyDialogue,
    user: UserProfile,
    msg: Message,
) -> HandlerResult {
    let menu = my_mensa_lib::get_menu(2).await?;

    // Extract explicit date argument, if present
    let explicit_date = msg
        .text()
        .and_then(|text| text.split_once(' '))
        .map(|(_, date)| date);

    let date = select_date(
        menu.iter().map(|dm| dm.date.as_str()).collect(),
        explicit_date,
    );

    if date.is_none() {
        dialogue.update(State::Idle { user }).await?;
        bot.send_message(msg.chat.id, "Error finding correct order date...")
            .await?;
        return Ok(());
    }
    let date = date.unwrap();

    let day_menu = menu.iter().find(|dm| dm.date == date).unwrap();

    let m = bot
        .send_message(msg.chat.id, format!("Choose Meal for {}", date))
        .reply_markup(make_menu_buttons(day_menu))
        .await?;

    dialogue
        .update(State::WaitingForOrderSelection {
            user,
            iso_date: day_menu.date.clone(),
            order_select_message: m.id,
        })
        .await?;

    Ok(())
}

async fn menu(bot: Bot, msg: Message) -> HandlerResult {
    let menu = my_mensa_lib::get_menu(2).await.unwrap();
    let mut reply = String::new();
    for day in menu {
        reply += format!("{}:\n", day.date).as_str();
        for item in day.meals {
            if item.combined_name.contains("Dessert") || item.combined_name.contains("Beilage") {
                continue;
            }
            reply += format!("  {}\n", item.combined_name).as_str();
        }
    }

    bot.send_message(msg.chat.id, reply).await?;

    Ok(())
}

fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    use dptree::case;

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(case![Command::Help].endpoint(help))
        .branch(case![Command::Start].endpoint(start))
        .branch(case![Command::Menu].endpoint(menu))
        .branch(case![State::Idle { user }].branch(case![Command::Order].endpoint(present_order)));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::WaitingForFirstName].endpoint(receive_first_name))
        .branch(case![State::WaitingForLastName { first_name }].endpoint(receive_last_name))
        .branch(
            case![State::ReceiveEmail {
                first_name,
                last_name
            }]
            .endpoint(receive_email),
        )
        .branch(dptree::endpoint(invalid_state));

    let callback_query_handler = Update::filter_callback_query()
        .branch(
            case![State::WaitingForOrderSelection {
                user,
                iso_date,
                order_select_message
            }]
            .endpoint(meal_select_callback),
        )
        .branch(
            case![State::WaitingForSlotSelection {
                user,
                iso_date,
                order_md5,
                slot_select_message
            }]
            .endpoint(slot_select_order_callback),
        );

    dialogue::enter::<Update, ErasedStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
}
