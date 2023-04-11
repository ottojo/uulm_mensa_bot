use my_mensa_lib::{get_menu, order, UserProfile};

pub fn main() {
    env_logger::init();

    let menu = get_menu(2).unwrap();
    for day in menu {
        println!("{}:", day.date);
        for item in day.meals {
            println!("  {} ({})", item.combined_name, item.md5);
        }
    }

    order(
        "2023-04-18",
        "0d2eddca77b52af25d308e0ac15fb30b",
        2,
        &UserProfile::new("a".to_owned(), "b".to_owned(), "c".to_owned()),
    )
    .unwrap();
}
