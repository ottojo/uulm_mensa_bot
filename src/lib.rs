use std::{
    collections::HashMap,
    fmt::format,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::Deserialize;
use serde_json::Value;

use log::debug;

const API_BASE_URL: &str = "https://stwulm.my-mensa.de";

#[derive(Deserialize, Debug)]
struct DayInfo {
    datum_iso: String,
    tag_formatiert2: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct MealAttributes {
    artikelId: String,
}

#[derive(Deserialize, Debug)]
struct Meal {
    title_clean: String,
    description_clean: String,
    category: String,
    md5: String,
    attributes: MealAttributes,
    kennzRest: String,
    title: String,
    description: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct Day {
    tag: DayInfo,
    essen: Vec<Meal>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
pub struct Data {
    mensaname: String,
    result: Vec<Day>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

pub struct UserProfile {
    firstname: String,
    lastname: String,
    email: String,
}

impl UserProfile {
    pub fn new(first: String, last: String, email: String) -> UserProfile {
        UserProfile {
            firstname: first,
            lastname: last,
            email,
        }
    }
}

pub fn order(iso_date: &str, md5: &str, mensa_id: i32, user: &UserProfile) -> Result<()> {
    let (cookie_store, menu_data) = get_menu_impl(mensa_id)?;

    let day = menu_data
        .result
        .into_iter()
        .find(|day| day.tag.datum_iso == iso_date)
        .expect(format!("Day not found in menu: {}", iso_date).as_str());

    let meal = day
        .essen
        .into_iter()
        .find(|m| m.md5 == md5)
        .expect(format!("Meal with md5 not found in menu: {}", md5).as_str());

    let a_id = meal.attributes.artikelId;

    let client = reqwest::blocking::Client::builder()
        .cookie_provider(cookie_store)
        .build()
        .context("Creating HTTP client failed")?;

    let mut params: HashMap<&str, &str> = HashMap::new();
    params.insert("client[einrichtung]", &menu_data.mensaname);

    let mensa_id_string = mensa_id.to_string();
    params.insert("client[einrichtung_val]", &mensa_id_string);

    params.insert("client[vorname]", &user.firstname);
    params.insert("client[name]", &user.lastname);
    params.insert("client[email]", &user.email);

    params.insert("client[nv2]", "true");
    params.insert("client[save_allowed]", "true");

    params.insert("client[deliver_time_val]", todo!()); // "12:00"
    params.insert("client[date_iso]", iso_date);
    params.insert("client[date_hr]", &day.tag.tag_formatiert2);

    params.insert(&format!("basket_positions[{a_id}]"), "1");
    params.insert("basket_html", todo!());

    let bf = format!("basket_full[{}]", a_id);
    params.insert((bf + "[id]").as_str(), &a_id);
    params.insert((bf + "[category]").as_str(), &meal.category);
    params.insert(
        (bf + "[title]").as_str(),
        &format!("{} {} {}", meal.title, meal.description, meal.kennzRest),
    );
    params.insert((bf + "[preis1]").as_str(), todo!());
    params.insert((bf + "[preis2]").as_str(), todo!());
    params.insert((bf + "[preis3]").as_str(), todo!());
    params.insert((bf + "[anzahl]").as_str(), todo!());

    // TODO
    client.post("url").form(&params).send()?;

    todo!();
}

pub struct MenuItem {
    pub category: String,
    pub name: String,
    pub combined_name: String,
    pub md5: String,
}

pub struct DayMenu {
    pub date: String,
    pub meals: Vec<MenuItem>,
}

fn get_menu_impl(mensa_id: i32) -> Result<(Arc<CookieStoreMutex>, Data)> {
    let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));

    let client = reqwest::blocking::Client::builder()
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .context("Creating HTTP client failed")?;

    let now = SystemTime::now();
    let since_the_epoch = now.duration_since(UNIX_EPOCH).unwrap();
    let now_millis = since_the_epoch.as_millis();

    let result = client
        .get(
            API_BASE_URL.to_owned()
                + format!(
                    "/getdata.php?mensa_id={mensa_id}&json=1&hyp=1&now={now_millis}&mode=togo&lang=de"
                )
                .as_str(),
        )
        .send()
        .context("Failed to make HTTP request")?;

    let json: Data = result.json().context("Failed to decode getdata response")?;

    debug!("Session cookies: {:?}", cookie_store.lock().unwrap());

    Ok((Arc::clone(&cookie_store), json))
}

pub fn get_menu(mensa_id: i32) -> Result<Vec<DayMenu>> {
    let (_, data) = get_menu_impl(mensa_id)?;

    Ok(data
        .result
        .into_iter()
        .map(|day| DayMenu {
            date: day.tag.datum_iso,
            meals: day
                .essen
                .into_iter()
                .map(|meal| MenuItem {
                    category: meal.category.clone(),
                    name: format!("{} {}", meal.title_clean, meal.description_clean),
                    combined_name: format!(
                        "{}: {}{}",
                        meal.category, meal.title_clean, meal.description_clean
                    ),
                    md5: meal.md5,
                })
                .collect(),
        })
        .collect())
}
