use my_mensa_lib::{get_free_slots, get_menu, order, UserProfile};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    mensa_id: i32,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Menu {},
    Slots {
        email: String,
        iso_date: String,
    },
    Order {
        iso_date: String,
        meal_md5: String,
        time: String,
        firstname: String,
        lastname: String,
        email: String,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Menu {} => {
            let menu = get_menu(2).await.unwrap();
            for day in menu {
                println!("{}:", day.date);
                for item in day.meals {
                    println!("  {} ({})", item.combined_name, item.md5);
                }
            }
        }
        Commands::Slots { email, iso_date } => {
            let slots = get_free_slots(cli.mensa_id, email.as_str(), iso_date.as_str())
                .await
                .unwrap();
            println!("Free slots for {}:", iso_date);
            for (time, count) in slots {
                println!("  {}: {}", time, count);
            }
        }
        Commands::Order {
            iso_date,
            meal_md5,
            time,
            firstname,
            lastname,
            email,
        } => {
            let res = order(
                &iso_date,
                &meal_md5,
                cli.mensa_id,
                &UserProfile::new(firstname, lastname, email),
                &time,
            )
            .await
            .unwrap();
            println!("{}", res);
        }
    }
}
