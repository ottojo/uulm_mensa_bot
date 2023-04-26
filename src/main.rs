use my_mensa_lib::DayMenu;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands,
};
use tokio::task::spawn_blocking;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().unwrap();
    pretty_env_logger::init();
    log::info!("Starting mensa bot...");

    let bot = Bot::from_env();

    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "set first name")]
    FirstName(String),
    #[command(description = "set last name")]
    LastName(String),
    #[command(description = "set email")]
    Email(String),
    #[command(description = "get this weeks menu")]
    Menu,
    #[command(description = "order")]
    Order,
    #[command()]
    FuckShitUp,
}

fn make_keyboard(menu: &DayMenu) -> InlineKeyboardMarkup {
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

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::FirstName(username) => {
            bot.send_message(msg.chat.id, format!("Your username is @{username}."))
                .await?
        }
        Command::LastName(name) => bot.send_message(msg.chat.id, "TODO: Set last name").await?,
        Command::Email(_) => bot.send_message(msg.chat.id, "TODO: Set email").await?,
        Command::Menu => {
            let menu = spawn_blocking(|| my_mensa_lib::get_menu(2))
                .await
                .unwrap()
                .unwrap();
            let mut reply = String::new();
            for day in menu {
                reply += format!("{}:\n", day.date).as_str();
                for item in day.meals {
                    reply += format!("  {}\n", item.combined_name).as_str();
                }
            }

            bot.send_message(msg.chat.id, reply).await?
        }
        Command::Order => {
            let menu = spawn_blocking(|| my_mensa_lib::get_menu(2))
                .await
                .unwrap()
                .unwrap();
            let keyboard = make_keyboard(&menu[0]);
            bot.send_message(msg.chat.id, "Order")
                .reply_markup(keyboard)
                .await?
        }
        Command::FuckShitUp => {
            bot.send_message(msg.chat.id, "Finger weg, Henning!")
                .await?
        }
    };

    Ok(())
}
