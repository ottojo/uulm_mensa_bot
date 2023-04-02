use anyhow::{Context, Result};
use serde::Deserialize;

const API_BASE_URL: &str = "https://stwulm.my-mensa.de";

#[derive(Deserialize, Debug)]
struct DayInfo {
    datum_iso: String,
}

#[derive(Deserialize, Debug)]
struct Meal {
    title_clean: String,
    description_clean: String,
    category: String,
}

#[derive(Deserialize, Debug)]
struct Day {
    tag: DayInfo,
    essen: Vec<Meal>,
}

#[derive(Deserialize, Debug)]
pub struct Data {
    mensaname: String,
    #[serde(rename = "result")]
    result: Vec<Day>,
}

pub fn test() -> Result<Data> {
    let result = reqwest::blocking::get(
        API_BASE_URL.to_owned()
            + "/getdata.php?mensa_id=2&json=1&hyp=1&now=1680449505846&mode=togo&lang=de",
    )
    .context("Failed to make HTTP request")?;

    let json: Data = result.json().context("Failed to decode getdata response")?;

    println!("Mensa \"{}\":", json.mensaname);
    for day in &json.result {
        println!("{}", day.tag.datum_iso);
        for meal in &day.essen {
            println!(
                "  {}: {}{}",
                meal.category, meal.title_clean, meal.description_clean
            );
        }
    }

    Ok(json)
}
