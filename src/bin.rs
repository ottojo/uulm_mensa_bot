use my_mensa_lib::{get_menu, order, UserProfile};

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
    Order {
        iso_date: String,
        meal_md5: String,
        time: String,
        firstname: String,
        lastname: String,
        email: String,
    },
}

pub fn main() {
    env_logger::init();

    let cli = Cli::parse();
    dbg!(&cli);

    match cli.command {
        Commands::Menu {} => {
            let menu = get_menu(2).unwrap();
            for day in menu {
                println!("{}:", day.date);
                for item in day.meals {
                    println!("  {} ({})", item.combined_name, item.md5);
                }
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
            .unwrap();
            println!("{}", res);
        }
    }
}
