use crate::model::{MenuGroup, TodayMenu};
use crate::restaurant::Provider;

#[derive(Debug, Clone, Copy)]
pub struct PriceGroups {
    pub student: bool,
    pub staff: bool,
    pub guest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriceGroup {
    Student,
    Staff,
    Guest,
}

#[derive(Debug, Clone)]
struct PriceEntry {
    group: PriceGroup,
    text: String,
    value: Option<f32>,
}

pub fn normalize_text(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for ch in value.chars() {
        let is_space = ch.is_whitespace();
        if is_space {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim().to_string()
}

pub fn normalize_optional(value: Option<&str>) -> String {
    match value {
        Some(v) => normalize_text(v),
        None => String::new(),
    }
}

pub fn format_display_date(date_iso: &str, language: &str) -> String {
    let iso = normalize_text(date_iso);
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() != 3 {
        return iso;
    }
    let year = parts[0];
    let month = match parts[1].parse::<u32>() {
        Ok(m) => m,
        Err(_) => return iso,
    };
    let day = match parts[2].parse::<u32>() {
        Ok(d) => d,
        Err(_) => return iso,
    };
    if language == "fi" {
        return format!("{}.{}.{}", day, month, year);
    }
    format!("{}/{}/{}", month, day, year)
}

pub fn date_and_time_line(today_menu: Option<&TodayMenu>, language: &str) -> String {
    let menu = match today_menu {
        Some(m) => m,
        None => return String::new(),
    };
    let date_part = format_display_date(&menu.date_iso, language);
    let time_part = normalize_text(&menu.lunch_time);
    if !date_part.is_empty() && !time_part.is_empty() {
        format!("{} {}", date_part, time_part)
    } else if !date_part.is_empty() {
        date_part
    } else {
        time_part
    }
}

pub fn text_for(language: &str, key: &str) -> String {
    if language == "fi" {
        match key {
            "loading" => "Ladataan ruokalistaa...".to_string(),
            "noMenu" => "Tälle päivälle ei ole lounaslistaa.".to_string(),
            "stale" => "Päivitys epäonnistui. Näytetään viimeisin tallennettu lista.".to_string(),
            "staleNetwork" => "Ei verkkoyhteyttä. Näytetään viimeisin tallennettu lista.".to_string(),
            "fetchError" => "Päivitysvirhe".to_string(),
            _ => key.to_string(),
        }
    } else {
        match key {
            "loading" => "Loading menu...".to_string(),
            "noMenu" => "No lunch menu available for today.".to_string(),
            "stale" => "Update failed. Showing last cached menu.".to_string(),
            "staleNetwork" => "Offline. Showing last cached menu.".to_string(),
            "fetchError" => "Fetch error".to_string(),
            _ => key.to_string(),
        }
    }
}

pub fn menu_heading(menu: &MenuGroup, provider: Provider, show_prices: bool, groups: PriceGroups) -> String {
    let mut heading = normalize_text(&menu.name);
    if heading.is_empty() {
        heading = "Menu".to_string();
    }
    let price = normalize_text(&menu.price);
    if show_prices && !price.is_empty() {
        if provider == Provider::Compass {
            let filtered = price_text_for_groups(&price, groups);
            if filtered.is_empty() {
                heading
            } else {
                format!("{} - {}", heading, filtered)
            }
        } else {
            format!("{} - {}", heading, price)
        }
    } else {
        heading
    }
}

pub fn split_component_suffix(component: &str) -> (String, String) {
    let text = normalize_text(component);
    if text.is_empty() {
        return (String::new(), String::new());
    }
    let trimmed = text.trim();
    if let Some(idx) = trimmed.rfind('(') {
        if trimmed.ends_with(')') {
            let (main, suffix) = trimmed.split_at(idx);
            let main = main.trim();
            let suffix = suffix.trim();
            let open_count = suffix.chars().filter(|c| *c == '(').count();
            let close_count = suffix.chars().filter(|c| *c == ')').count();
            if open_count == 1 && close_count == 1 && !main.is_empty() {
                return (normalize_text(main), normalize_text(suffix));
            }
        }
    }
    (trimmed.to_string(), String::new())
}

pub fn student_price_eur(price: &str) -> Option<f32> {
    let entries = parse_compass_price_entries(price);
    entries
        .into_iter()
        .find(|entry| entry.group == PriceGroup::Student)
        .and_then(|entry| entry.value)
}

fn price_text_for_groups(price: &str, groups: PriceGroups) -> String {
    let entries = parse_compass_price_entries(price);
    let mut parts = Vec::new();
    for entry in entries {
        let include = match entry.group {
            PriceGroup::Student => groups.student,
            PriceGroup::Staff => groups.staff,
            PriceGroup::Guest => groups.guest,
        };
        if include {
            parts.push(entry.text);
        }
    }
    parts.join(" / ")
}

fn parse_compass_price_entries(price: &str) -> Vec<PriceEntry> {
    let normalized = normalize_text(price);
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut entries = Vec::new();
    for segment in normalized.split('/') {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }
        let lower = seg.to_lowercase();
        let group = if contains_any(&lower, &["student", "op", "opisk", "opiskelija"]) {
            PriceGroup::Student
        } else if contains_any(&lower, &["staff", "hk", "henkilokunta", "henkilökunta"]) {
            PriceGroup::Staff
        } else if contains_any(&lower, &["guest", "vieras"]) {
            PriceGroup::Guest
        } else {
            PriceGroup::Guest
        };
        entries.push(PriceEntry {
            group,
            text: seg.to_string(),
            value: parse_price_value(seg),
        });
    }
    entries
}

fn contains_any(text: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| text.contains(token))
}

fn parse_price_value(text: &str) -> Option<f32> {
    let mut current = String::new();
    let mut tokens: Vec<String> = Vec::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == ',' || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    let token = tokens.last()?.replace(',', ".");
    let cleaned = token.trim_matches('.');
    cleaned.parse::<f32>().ok()
}
