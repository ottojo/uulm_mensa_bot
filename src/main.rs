use log::warn;
use my_mensa_lib::{DayMenu, LinkedHashMap, UserProfile};
use std::future::IntoFuture;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId},
    utils::command::BotCommands,
};
use tokio::join;

const STAGING: bool = true;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().unwrap();
    pretty_env_logger::init();
    log::info!("Starting mensa bot...");

    let bot = Bot::from_env();

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![InMemStorage::<State>::new()])
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
    #[command(description = "display this text.")]
    Help,
    #[command(description = "restart welcome dialog")]
    Start,
    #[command(description = "get this weeks menu")]
    Menu,
    #[command(description = "order")]
    Order,
}

#[derive(Clone, Default, Debug)]
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

type MyDialogue = Dialogue<State, InMemStorage<State>>;
type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, "Hello! Please enter your first name.")
        .await?;
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

    dialogue
        .update(State::Idle {
            user: UserProfile::new(first_name, last_name, email.to_owned()),
        })
        .await?;

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

async fn meal_select_callback(
    bot: Bot,
    dialogue: MyDialogue,
    (user, iso_date, order_select_message): (UserProfile, String, MessageId),
    q: CallbackQuery,
) -> HandlerResult {
    let slots = my_mensa_lib::get_free_slots(2, &user.email, &iso_date).await?;

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
        format!("Ordering {:?} for {:?}", q.data, user),
    )
    .await?;

    let selected_slot = q.data.unwrap();

    if STAGING {
        log::info!("STAGING: Not actually ordering anything");
    } else {
        my_mensa_lib::order(iso_date.as_str(), &order_md5, 2, &user, &selected_slot).await?;
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

async fn present_order(
    bot: Bot,
    dialogue: MyDialogue,
    user: UserProfile,
    msg: Message,
) -> HandlerResult {
    let menu = my_mensa_lib::get_menu(2).await?;
    let m = bot
        .send_message(msg.chat.id, "Choose Meal")
        .reply_markup(make_menu_buttons(&menu[0]))
        .await?;

    dialogue
        .update(State::WaitingForOrderSelection {
            user,
            iso_date: menu[0].date.clone(),
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

    // TODO: Allow in idle

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

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
}
