use crate::model::{MenuGroup, TodayMenu};

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
            "noMenu" => "Talle paivalle ei ole lounaslistaa.".to_string(),
            "stale" => "Ei verkkoyhteytta. Naytetaan viimeisin tallennettu lista".to_string(),
            "fetchError" => "Paivitysvirhe".to_string(),
            _ => key.to_string(),
        }
    } else {
        match key {
            "loading" => "Loading menu...".to_string(),
            "noMenu" => "No lunch menu available for today.".to_string(),
            "stale" => "Offline. Showing last cached menu".to_string(),
            "fetchError" => "Fetch error".to_string(),
            _ => key.to_string(),
        }
    }
}

pub fn menu_heading(menu: &MenuGroup, show_prices: bool) -> String {
    let mut heading = normalize_text(&menu.name);
    if heading.is_empty() {
        heading = "Menu".to_string();
    }
    let price = normalize_text(&menu.price);
    if show_prices && !price.is_empty() {
        format!("{} - {}", heading, price)
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
