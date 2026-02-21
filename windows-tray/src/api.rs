use crate::antell;
use crate::format::{normalize_optional, normalize_text};
use crate::model::{ApiResponse, ApiSetMenu, MenuGroup, TodayMenu};
use crate::restaurant::{restaurant_for_code, Provider, Restaurant};
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
    pub provider: Provider,
    pub raw_json: String,
    pub payload_date: String,
}

pub fn fetch_today(settings: &Settings) -> FetchOutput {
    let restaurant = restaurant_for_code(&settings.restaurant_code, settings.enable_antell_restaurants);
    match restaurant.provider {
        Provider::Compass => fetch_compass(settings, restaurant),
        Provider::Antell => fetch_antell(restaurant),
    }
}

fn fetch_compass(settings: &Settings, restaurant: Restaurant) -> FetchOutput {
    let url = format!(
        "https://www.compass-group.fi/menuapi/feed/json?costNumber={}&language={}",
        restaurant.code, settings.language
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
                provider: Provider::Compass,
                raw_json: String::new(),
                payload_date: String::new(),
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
                            provider: Provider::Compass,
                            raw_json,
                            payload_date: String::new(),
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
                    provider: Provider::Compass,
                    raw_json,
                    payload_date: String::new(),
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
                provider: Provider::Compass,
                raw_json,
                payload_date: String::new(),
            };
        }
    };

    parse_response(api, raw_json)
}

pub fn parse_cached_payload(
    raw_payload: &str,
    provider: Provider,
    restaurant: Restaurant,
) -> anyhow::Result<FetchOutput> {
    match provider {
        Provider::Compass => {
            let api: ApiResponse = serde_json::from_str(raw_payload).context("parse cached JSON")?;
            Ok(parse_response(api, raw_payload.to_string()))
        }
        Provider::Antell => {
            let today_key = local_today_key();
            let today_menu = antell::parse_antell_html(raw_payload, &today_key);
            Ok(FetchOutput {
                ok: true,
                error_message: String::new(),
                today_menu: Some(today_menu),
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider,
                raw_json: raw_payload.to_string(),
                payload_date: String::new(),
            })
        }
    }
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
            provider: Provider::Compass,
            raw_json,
            payload_date: String::new(),
        };
    }

    let today_key = local_today_key();
    let menus_for_days = api.menus_for_days.unwrap_or_default();
    let mut today_menu: Option<TodayMenu> = None;
    let mut payload_date = String::new();

    for day in menus_for_days {
        let date_key = normalize_optional(day.date.as_deref())
            .split('T')
            .next()
            .unwrap_or("")
            .to_string();
        if !date_key.is_empty() && (payload_date.is_empty() || date_key > payload_date) {
            payload_date = date_key.clone();
        }
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
        provider: Provider::Compass,
        raw_json,
        payload_date,
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

fn fetch_antell(restaurant: Restaurant) -> FetchOutput {
    let today_key = local_today_key();
    let slug = match restaurant.antell_slug {
        Some(s) => s,
        None => {
            return FetchOutput {
                ok: false,
                error_message: "Missing Antell slug".to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };
    let url = format!("https://antell.fi/lounas/kuopio/{}/?print_lunch_day={}&print_lunch_list_day=1", slug, weekday_token());
    let client = match Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    let response = client.get(&url).send();
    match response {
        Ok(mut resp) => match resp.text() {
            Ok(text) => {
                let today_menu = antell::parse_antell_html(&text, &today_key);
                FetchOutput {
                    ok: true,
                    error_message: String::new(),
                    today_menu: Some(today_menu),
                    restaurant_name: restaurant.name.to_string(),
                    restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                    provider: Provider::Antell,
                    raw_json: text,
                    payload_date: today_key,
                }
            }
            Err(err) => FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
                raw_json: String::new(),
                payload_date: String::new(),
            },
        },
        Err(err) => FetchOutput {
            ok: false,
            error_message: err.to_string(),
            today_menu: None,
            restaurant_name: restaurant.name.to_string(),
            restaurant_url: restaurant.url.unwrap_or_default().to_string(),
            provider: Provider::Antell,
            raw_json: String::new(),
            payload_date: String::new(),
        },
    }
}

fn weekday_token() -> &'static str {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    match now.weekday() {
        time::Weekday::Monday => "monday",
        time::Weekday::Tuesday => "tuesday",
        time::Weekday::Wednesday => "wednesday",
        time::Weekday::Thursday => "thursday",
        time::Weekday::Friday => "friday",
        time::Weekday::Saturday => "saturday",
        time::Weekday::Sunday => "sunday",
    }
}

fn local_today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!("{:04}-{:02}-{:02}", date.year(), date.month() as u8, date.day())
}
