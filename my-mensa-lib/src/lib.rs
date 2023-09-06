use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
pub use linked_hash_map::LinkedHashMap;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::Deserialize;

use log::debug;

const API_BASE_URL: &str = "https://stwulm.my-mensa.de";

#[derive(Deserialize, Debug)]
struct DayInfo {
    datum_iso: String,
    tag_formatiert2: String,
}

#[derive(Deserialize, Debug)]
struct MealAttributes {
    #[serde(rename = "artikelId")]
    artikel_id: String,
}

#[derive(Deserialize, Debug)]
struct Meal {
    title_clean: String,
    description_clean: String,
    category: String,
    md5: String,
    attributes: MealAttributes,
    #[serde(rename = "kennzRest")]
    kennz_rest: String,
    title: String,
    description: String,
    preis1: String,
    preis2: String,
    preis3: String,
    #[serde(rename = "preis_formated_Togo")]
    preis_formated_togo: String,
}

#[derive(Deserialize, Debug)]
struct Day {
    tag: DayInfo,
    essen: Vec<Meal>,
}

#[derive(Deserialize, Debug)]
pub struct Data {
    mensaname: String,
    result: Vec<Day>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub firstname: String,
    pub lastname: String,
    pub email: String,
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

pub async fn get_free_slots(
    mensa_id: i32,
    email: &str,
    iso_date: &str,
) -> Result<LinkedHashMap<String, i32>> {
    let client = reqwest::Client::builder()
        .build()
        .context("Creating HTTP client failed")?;

    let mut params: HashMap<&str, &str> = HashMap::new();
    let mensa_id_str = mensa_id.to_string();
    params.insert("mensa_id", &mensa_id_str);
    params.insert("tag", iso_date);
    params.insert("id", email);

    let url = "https://togo.my-mensa.de/5ecb878c-9f58-4aa0-bb1b/ulm19c552/api/get_free_slots/";
    log::trace!("Calling API url: {}", &url);

    let result = client.post(url).form(&params).send().await.context("")?;
    log::trace!("Response: {:?}", &result);

    let response_text = result.text().await?;
    log::trace!("Text: {:?}", response_text);

    let json =
        serde_json::from_str(&response_text).context("Failed to decode get_free_slots response")?;

    Ok(json)
}

pub async fn order(
    iso_date: &str,
    md5: &str,
    mensa_id: i32,
    user: &UserProfile,
    time: &str,
) -> Result<String> {
    let (cookie_store, menu_data) = get_menu_impl(mensa_id).await?;

    let day = menu_data
        .result
        .into_iter()
        .find(|day| day.tag.datum_iso == iso_date)
        .unwrap_or_else(|| panic!("Day not found in menu: {}", iso_date));

    let meal = day
        .essen
        .into_iter()
        .find(|m| m.md5 == md5)
        .unwrap_or_else(|| panic!("Meal with md5 not found in menu: {}", md5));

    let slots = get_free_slots(mensa_id, &user.email, iso_date).await?;
    let (slot_time, slot_free) = slots
        .into_iter()
        .find(|(k, _v)| k.starts_with(time))
        .ok_or(anyhow!("Time slot not found"))?;

    if slot_free <= 0 {
        return Err(anyhow!("Time slot full!"));
    }

    let slot_time = &slot_time[..5];

    let a_id = meal.attributes.artikel_id;

    let client = reqwest::Client::builder()
        .cookie_provider(cookie_store)
        .build()
        .context("Creating HTTP client failed")?;

    let mut params: HashMap<String, String> = HashMap::new();
    params.insert("client[einrichtung]".to_owned(), menu_data.mensaname);

    let mensa_id_string = mensa_id.to_string();
    params.insert("client[einrichtung_val]".to_owned(), mensa_id_string);

    params.insert("client[vorname]".to_owned(), user.firstname.clone());
    params.insert("client[name]".to_owned(), user.lastname.clone());
    params.insert("client[email]".to_owned(), user.email.clone());

    params.insert("client[nv2]".to_owned(), "true".to_owned());
    params.insert("client[save_allowed]".to_owned(), "true".to_owned());

    params.insert("client[deliver_time_val]".to_owned(), slot_time.to_owned());
    params.insert("client[date_iso]".to_owned(), iso_date.to_owned());
    params.insert("client[date_hr]".to_owned(), day.tag.tag_formatiert2);

    params.insert(format!("basket_positions[{a_id}]"), "1".to_owned());

    let title = meal.title;
    let preis_formated_togo = meal.preis_formated_togo;

    let auflistung_html = format!("<tbody><tr><th>Anzahl</th> <th>Artikel</th> <th class=\"zahl\">Stückpreis</th></tr> <tr><td>1x</td> <td aid_check=\"{a_id}\">{title}</td> <td class=\"preis\">{preis_formated_togo}</td></tr> <tr class=\"trenner\"><td></td> <td></td> <td></td></tr></tbody>");

    params.insert("basket_html".to_owned(), auflistung_html);

    let bf = format!("basket_full[{}]", a_id);
    params.insert(bf.clone() + "[id]", a_id);
    params.insert(bf.clone() + "[category]", meal.category);
    params.insert(
        bf.clone() + "[title]",
        format!("{} {} {}", title, meal.description, meal.kennz_rest),
    );
    params.insert(bf.clone() + "[preis1]", meal.preis1);
    params.insert(bf.clone() + "[preis2]", meal.preis2);
    params.insert(bf.clone() + "[preis3]", meal.preis3);
    params.insert(bf + "[anzahl]", "1".to_owned());

    let response: String = client
        .post("https://stwulm.my-mensa.de/setDataMensaTogo.php?order=add&language=de")
        .form(&params)
        .send()
        .await?
        .text()
        .await?;

    Ok(response)
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

async fn get_menu_impl(mensa_id: i32) -> Result<(Arc<CookieStoreMutex>, Data)> {
    let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));

    let client = reqwest::Client::builder()
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .context("Creating HTTP client failed")?;

    let now = SystemTime::now();
    let since_the_epoch = now.duration_since(UNIX_EPOCH).unwrap();
    let now_millis = since_the_epoch.as_millis();

    let url: String = format!("{API_BASE_URL}/getdata.php?mensa_id={mensa_id}&json=1&hyp=1&now={now_millis}&mode=togo&lang=de");
    log::trace!("Calling API url: {}", &url);

    let result = client
        .get(url.as_str())
        .send()
        .await
        .context("Failed to make HTTP request")?;
    log::trace!("Response: {:?}", &result);

    let response_text = result.text().await?;
    log::trace!("Text: {:?}", response_text);

    let json: Data =
        serde_json::from_str(&response_text).context("Failed to decode getdata response")?;

    debug!("Session cookies: {:?}", cookie_store.lock().unwrap());

    Ok((Arc::clone(&cookie_store), json))
}

pub async fn get_menu(mensa_id: i32) -> Result<Vec<DayMenu>> {
    let (_, data) = get_menu_impl(mensa_id).await?;

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
