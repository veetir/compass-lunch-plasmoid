use crate::format::{normalize_optional, normalize_text};
use crate::model::{ApiResponse, ApiSetMenu, MenuGroup, TodayMenu};
use crate::settings::Settings;
use anyhow::Context;
use reqwest::blocking::Client;
use time::OffsetDateTime;

pub struct FetchOutput {
    pub ok: bool,
    pub error_message: String,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub raw_json: String,
}

pub fn fetch_today(settings: &Settings) -> FetchOutput {
    let url = format!(
        "https://www.compass-group.fi/menuapi/feed/json?costNumber={}&language={}",
        settings.restaurant_code, settings.language
    );

    let client = match Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: String::new(),
                restaurant_url: String::new(),
                raw_json: String::new(),
            };
        }
    };

    let response = client.get(&url).send();
    let mut raw_json = String::new();
    let api: ApiResponse = match response {
        Ok(mut resp) => match resp.text() {
            Ok(text) => {
                raw_json = text.clone();
                match serde_json::from_str(&text) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        return FetchOutput {
                            ok: false,
                            error_message: err.to_string(),
                            today_menu: None,
                            restaurant_name: String::new(),
                            restaurant_url: String::new(),
                            raw_json,
                        };
                    }
                }
            }
            Err(err) => {
                return FetchOutput {
                    ok: false,
                    error_message: err.to_string(),
                    today_menu: None,
                    restaurant_name: String::new(),
                    restaurant_url: String::new(),
                    raw_json,
                };
            }
        },
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: String::new(),
                restaurant_url: String::new(),
                raw_json,
            };
        }
    };

    parse_response(api, raw_json)
}

pub fn parse_cached_payload(raw_json: &str) -> anyhow::Result<FetchOutput> {
    let api: ApiResponse = serde_json::from_str(raw_json).context("parse cached JSON")?;
    Ok(parse_response(api, raw_json.to_string()))
}

fn parse_response(api: ApiResponse, raw_json: String) -> FetchOutput {
    let error_text = normalize_optional(api.error_text.as_deref());
    if !error_text.is_empty() {
        return FetchOutput {
            ok: false,
            error_message: error_text,
            today_menu: None,
            restaurant_name: normalize_optional(api.restaurant_name.as_deref()),
            restaurant_url: normalize_optional(api.restaurant_url.as_deref()),
            raw_json,
        };
    }

    let today_key = local_today_key();
    let menus_for_days = api.menus_for_days.unwrap_or_default();
    let mut today_menu: Option<TodayMenu> = None;

    for day in menus_for_days {
        let date_key = normalize_optional(day.date.as_deref()).split('T').next().unwrap_or("").to_string();
        if date_key == today_key {
            let lunch_time = normalize_optional(day.lunch_time.as_deref());
            let set_menus = day.set_menus.unwrap_or_default();
            let menus = normalize_menus(set_menus);
            today_menu = Some(TodayMenu {
                date_iso: today_key.clone(),
                lunch_time,
                menus,
            });
            break;
        }
    }

    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu,
        restaurant_name: normalize_optional(api.restaurant_name.as_deref()),
        restaurant_url: normalize_optional(api.restaurant_url.as_deref()),
        raw_json,
    }
}

fn normalize_menus(set_menus: Vec<ApiSetMenu>) -> Vec<MenuGroup> {
    let mut menus_with_idx: Vec<(usize, ApiSetMenu)> = set_menus.into_iter().enumerate().collect();
    let has_sort = menus_with_idx.iter().any(|(_, m)| m.sort_order.is_some());
    if has_sort {
        menus_with_idx.sort_by_key(|(idx, menu)| (menu.sort_order.unwrap_or(*idx as i32), *idx as i32));
    }
    menus_with_idx
        .into_iter()
        .map(|(_, menu)| MenuGroup {
            name: normalize_optional(menu.name.as_deref()),
            price: normalize_optional(menu.price.as_deref()),
            components: menu
                .components
                .unwrap_or_default()
                .into_iter()
                .map(|c| normalize_text(&c))
                .filter(|c| !c.is_empty())
                .collect(),
        })
        .collect()
}

fn local_today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!("{:04}-{:02}-{:02}", date.year(), date.month() as u8, date.day())
}
