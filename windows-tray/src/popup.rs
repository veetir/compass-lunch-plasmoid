use crate::api;
use crate::app::{AppState, FetchStatus};
use crate::cache;
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, split_component_suffix, student_price_eur,
    text_for, PriceGroups,
};
use crate::model::TodayMenu;
use crate::restaurant::{available_restaurants, Provider, Restaurant};
use crate::settings::Settings;
use crate::util::to_wstring;
use std::sync::{Mutex, OnceLock};
use time::{OffsetDateTime, UtcOffset};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetDeviceCaps,
    GetMonitorInfoW, GetTextExtentPoint32W, GetTextMetricsW, InvalidateRect, MonitorFromPoint,
    SelectObject, SetBkMode, SetTextColor, TextOutW, HDC, HFONT, LOGPIXELSY, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, SetWindowPos, ShowWindow, HWND_TOPMOST,
    SWP_SHOWWINDOW, SW_HIDE,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 10;
const POPUP_WIDTH: i32 = 525;
const HEADER_HEIGHT: i32 = 46;
const HEADER_BUTTON_SIZE: i32 = 30;
const HEADER_BUTTON_GAP: i32 = 8;
const LOADING_HINT_DELAY_MS: i64 = 250;
const MAX_DYNAMIC_LINES: usize = 35;

static POPUP_LINE_BUDGET_CACHE: OnceLock<Mutex<Option<PopupLineBudgetCache>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupLineBudgetKey {
    today_key: String,
    language: String,
    enable_antell_restaurants: bool,
    show_prices: bool,
    show_student_price: bool,
    show_staff_price: bool,
    show_guest_price: bool,
    hide_expensive_student_meals: bool,
    show_allergens: bool,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RestaurantCacheSignature {
    code: String,
    mtime_ms: i64,
}

#[derive(Debug, Clone)]
struct PopupLineBudgetCache {
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
    max_lines: Option<usize>,
}

#[derive(Debug, Clone)]
enum Line {
    Heading(String),
    Text(String),
    TextWithSuffixSegments {
        main: String,
        segments: Vec<(String, bool)>,
    },
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderButtonAction {
    Prev,
    Next,
    Close,
}

#[derive(Debug, Clone, Copy)]
struct HeaderLayout {
    prev: RECT,
    next: RECT,
    close: RECT,
}

pub fn toggle_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        if is_visible(hwnd) {
            ShowWindow(hwnd, SW_HIDE);
        } else {
            show_popup(hwnd, state);
        }
    }
}

pub fn show_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let (x, y) = position_near_point(width, height, cursor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_tray_rect(width, height, tray_rect);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn resize_popup_keep_position(hwnd: HWND, state: &AppState) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            show_popup(hwnd, state);
            return;
        }
        let (width, height) = desired_size(hwnd, state);
        let anchor = POINT {
            x: rect.right,
            y: rect.bottom,
        };
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn hide_popup(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn header_button_at(hwnd: HWND, x: i32, y: i32) -> Option<HeaderButtonAction> {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let width = rect.right - rect.left;
        let layout = header_layout(width);
        if point_in_rect(&layout.prev, x, y) {
            return Some(HeaderButtonAction::Prev);
        }
        if point_in_rect(&layout.next, x, y) {
            return Some(HeaderButtonAction::Next);
        }
        if point_in_rect(&layout.close, x, y) {
            return Some(HeaderButtonAction::Close);
        }
        None
    }
}

pub fn paint_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.0 == 0 {
            return;
        }

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let (bg_color, text_color, suffix_color, header_bg_color, button_bg_color, divider_color) =
            theme_colors(&state.settings.theme);
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        SetBkMode(hdc, TRANSPARENT);

        let (normal_font, bold_font, small_font, small_bold_font) = create_fonts(hdc);
        let _old_font = SelectObject(hdc, normal_font);

        let lines = build_lines(state);
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;
        let content_width = (width - PADDING_X * 2).max(40);

        let header_rect = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.top + HEADER_HEIGHT,
        };
        let header_brush = CreateSolidBrush(header_bg_color);
        FillRect(hdc, &header_rect, header_brush);
        DeleteObject(header_brush);

        let layout = header_layout(width);
        draw_header_button(
            hdc,
            &layout.prev,
            "<",
            button_bg_color,
            text_color,
            normal_font,
        );
        draw_header_button(
            hdc,
            &layout.next,
            ">",
            button_bg_color,
            text_color,
            normal_font,
        );
        draw_header_button(
            hdc,
            &layout.close,
            "X",
            button_bg_color,
            text_color,
            normal_font,
        );

        SelectObject(hdc, bold_font);
        SetTextColor(hdc, text_color);
        let title = fit_text_to_width(
            hdc,
            &header_title(state),
            (layout.close.left - layout.next.right - 24).max(40),
        );
        let title_width = text_width(hdc, &title);
        let title_x = ((width - title_width) / 2).max(layout.next.right + 12);
        let title_y = header_rect.top + (HEADER_HEIGHT - metrics.tmHeight as i32) / 2 - 1;
        draw_text_line(hdc, &title, title_x, title_y);

        let divider_rect = RECT {
            left: rect.left,
            top: header_rect.bottom - 1,
            right: rect.right,
            bottom: header_rect.bottom,
        };
        let divider_brush = CreateSolidBrush(divider_color);
        FillRect(hdc, &divider_rect, divider_brush);
        DeleteObject(divider_brush);

        let mut y = HEADER_HEIGHT + PADDING_Y;
        for line in lines {
            match line {
                Line::Heading(text) => {
                    SelectObject(hdc, bold_font);
                    SetTextColor(hdc, text_color);
                    let clipped = fit_text_to_width(hdc, &text, content_width);
                    draw_text_line(hdc, &clipped, PADDING_X, y);
                    y += line_height;
                }
                Line::Text(text) => {
                    SelectObject(hdc, normal_font);
                    SetTextColor(hdc, text_color);
                    let clipped = fit_text_to_width(hdc, &text, content_width);
                    draw_text_line(hdc, &clipped, PADDING_X, y);
                    y += line_height;
                }
                Line::TextWithSuffixSegments { main, segments } => {
                    SelectObject(hdc, normal_font);
                    SetTextColor(hdc, text_color);
                    let mut suffix_width = 0;
                    for (segment, bold) in &segments {
                        let font = if *bold { small_bold_font } else { small_font };
                        SelectObject(hdc, font);
                        suffix_width += text_width(hdc, segment);
                    }
                    let max_main = (content_width - suffix_width - 4).max(24);
                    SelectObject(hdc, normal_font);
                    let clipped_main = fit_text_to_width(hdc, &main, max_main);
                    let main_width = text_width(hdc, &clipped_main);
                    draw_text_line(hdc, &clipped_main, PADDING_X, y);
                    if !segments.is_empty() {
                        let suffix_x = PADDING_X + main_width + 4;
                        if suffix_x < (PADDING_X + content_width) {
                            draw_text_segments(
                                hdc,
                                &segments,
                                suffix_x,
                                y + 1,
                                small_font,
                                small_bold_font,
                                suffix_color,
                            );
                        }
                    }
                    y += line_height;
                }
                Line::Spacer => {
                    y += line_height / 2;
                }
            }
        }

        SelectObject(hdc, _old_font);
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        EndPaint(hwnd, &ps);
    }
}

fn draw_text_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    x: i32,
    y: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    color: COLORREF,
) {
    let mut cursor = x;
    for (text, bold) in segments {
        let font = if *bold { bold_font } else { normal_font };
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(hdc, color);
        }
        draw_text_line(hdc, text, cursor, y);
        cursor += text_width(hdc, text);
    }
}

fn draw_text_line(hdc: HDC, text: &str, x: i32, y: i32) {
    let wide = to_wstring(text);
    unsafe {
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = TextOutW(hdc, x, y, slice);
        }
    }
}

fn fit_text_to_width(hdc: HDC, text: &str, max_width: i32) -> String {
    let clean = normalize_text(text);
    if clean.is_empty() || max_width <= 0 {
        return String::new();
    }
    if text_width(hdc, &clean) <= max_width {
        return clean;
    }

    let ellipsis = "...";
    let ellipsis_width = text_width(hdc, ellipsis);
    if ellipsis_width >= max_width {
        return ellipsis.to_string();
    }

    let mut out = String::new();
    for ch in clean.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);
        candidate.push_str(ellipsis);
        if text_width(hdc, &candidate) > max_width {
            break;
        }
        out.push(ch);
    }

    let mut trimmed = out.trim_end().to_string();
    trimmed.push_str(ellipsis);
    trimmed
}

fn draw_header_button(
    hdc: HDC,
    rect: &RECT,
    label: &str,
    bg_color: COLORREF,
    text_color: COLORREF,
    font: HFONT,
) {
    unsafe {
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, rect, brush);
        DeleteObject(brush);
        SelectObject(hdc, font);
        SetTextColor(hdc, text_color);
    }
    let label_width = text_width(hdc, label);
    let metrics = text_metrics(hdc, font);
    let x = rect.left + ((rect.right - rect.left - label_width) / 2).max(0);
    let y = rect.top + ((rect.bottom - rect.top - metrics.tmHeight as i32) / 2).max(0);
    draw_text_line(hdc, label, x, y);
}

fn header_layout(width: i32) -> HeaderLayout {
    let top = (HEADER_HEIGHT - HEADER_BUTTON_SIZE) / 2;
    let prev = RECT {
        left: PADDING_X,
        top,
        right: PADDING_X + HEADER_BUTTON_SIZE,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    let next = RECT {
        left: prev.right + HEADER_BUTTON_GAP,
        top,
        right: prev.right + HEADER_BUTTON_GAP + HEADER_BUTTON_SIZE,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    let close = RECT {
        left: width - PADDING_X - HEADER_BUTTON_SIZE,
        top,
        right: width - PADDING_X,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    HeaderLayout { prev, next, close }
}

fn header_title(state: &AppState) -> String {
    let list = available_restaurants(state.settings.enable_antell_restaurants);
    if list.is_empty() {
        return "Compass Lunch".to_string();
    }

    let index = list
        .iter()
        .position(|entry| entry.code == state.settings.restaurant_code)
        .unwrap_or(0);
    format!("{} ({}/{})", list[index].name, index + 1, list.len())
}

fn text_metrics(hdc: HDC, font: HFONT) -> TEXTMETRICW {
    unsafe {
        let old = SelectObject(hdc, font);
        let mut metrics = TEXTMETRICW::default();
        GetTextMetricsW(hdc, &mut metrics);
        SelectObject(hdc, old);
        metrics
    }
}

fn text_width(hdc: HDC, text: &str) -> i32 {
    let wide = to_wstring(text);
    unsafe {
        let mut size = windows::Win32::Foundation::SIZE::default();
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = GetTextExtentPoint32W(hdc, slice, &mut size);
        }
        size.cx
    }
}

fn desired_size(hwnd: HWND, state: &AppState) -> (i32, i32) {
    unsafe {
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let (normal_font, bold_font, small_font, small_bold_font) = create_fonts(hdc);
        let current_lines = build_lines(state);
        let target_lines = popup_target_line_count(state, current_lines.len());
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;
        let height = HEADER_HEIGHT + (target_lines as i32 * line_height) + PADDING_Y * 2;
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        (POPUP_WIDTH, height.max(HEADER_HEIGHT + 120))
    }
}

fn create_fonts(hdc: HDC) -> (HFONT, HFONT, HFONT, HFONT) {
    unsafe {
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY);
        let height_normal = -MulDiv(12, dpi, 72);
        let height_small = -MulDiv(10, dpi, 72);
        let face = to_wstring("Segoe UI");

        let normal = CreateFontW(
            height_normal,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let bold = CreateFontW(
            height_normal,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let small = CreateFontW(
            height_small,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let small_bold = CreateFontW(
            height_small,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        (normal, bold, small, small_bold)
    }
}

fn build_lines(state: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();

    let mut restaurant = normalize_text(&state.restaurant_name);
    if state.stale_date {
        if restaurant.is_empty() {
            restaurant = "[STALE]".to_string();
        } else {
            restaurant = format!("[STALE] {}", restaurant);
        }
    }
    if !restaurant.is_empty() {
        lines.push(Line::Heading(restaurant));
    }

    let show_loading_hint = state.status == FetchStatus::Loading
        && state.today_menu.is_none()
        && state.loading_started_epoch_ms > 0
        && now_epoch_ms().saturating_sub(state.loading_started_epoch_ms) >= LOADING_HINT_DELAY_MS;

    if show_loading_hint {
        lines.push(Line::Text(text_for(&state.settings.language, "loading")));
    }

    let date_line = date_and_time_line(state.today_menu.as_ref(), &state.settings.language);
    if !date_line.is_empty() {
        lines.push(Line::Heading(date_line));
    }

    match &state.today_menu {
        Some(menu) => {
            if !menu.menus.is_empty() {
                let price_groups = PriceGroups {
                    student: state.settings.show_student_price,
                    staff: state.settings.show_staff_price,
                    guest: state.settings.show_guest_price,
                };
                append_menus(
                    &mut lines,
                    menu,
                    state.provider,
                    state.settings.show_prices,
                    price_groups,
                    state.settings.show_allergens,
                    state.settings.highlight_gluten_free,
                    state.settings.highlight_veg,
                    state.settings.highlight_lactose_free,
                    state.settings.hide_expensive_student_meals,
                );
            } else if state.status != FetchStatus::Loading {
                lines.push(Line::Text(text_for(&state.settings.language, "noMenu")));
            }
        }
        None => {
            if state.status != FetchStatus::Loading {
                lines.push(Line::Text(text_for(&state.settings.language, "noMenu")));
            }
        }
    }

    if state.status == FetchStatus::Stale {
        lines.push(Line::Spacer);
        let stale_key = if state.stale_network_error {
            "staleNetwork"
        } else {
            "stale"
        };
        lines.push(Line::Text(text_for(&state.settings.language, stale_key)));
    }

    if !state.error_message.is_empty() && state.status != FetchStatus::Ok {
        lines.push(Line::Text(format!(
            "{}: {}",
            text_for(&state.settings.language, "fetchError"),
            state.error_message
        )));
    }

    lines
}

fn popup_target_line_count(state: &AppState, current_lines: usize) -> usize {
    let today_key = local_today_key();
    let key = line_budget_key(&state.settings, &today_key);
    let signatures = cache_signatures(&state.settings);

    if let Some(max_lines) = cached_line_budget(&key, &signatures) {
        return max_lines.unwrap_or(current_lines).min(MAX_DYNAMIC_LINES);
    }

    let max_lines = max_today_cached_line_count(state, &today_key);
    update_line_budget_cache(key, signatures, max_lines);
    match max_lines {
        Some(count) => count.min(MAX_DYNAMIC_LINES),
        None => current_lines.min(MAX_DYNAMIC_LINES),
    }
}

fn line_budget_key(settings: &Settings, today_key: &str) -> PopupLineBudgetKey {
    PopupLineBudgetKey {
        today_key: today_key.to_string(),
        language: settings.language.clone(),
        enable_antell_restaurants: settings.enable_antell_restaurants,
        show_prices: settings.show_prices,
        show_student_price: settings.show_student_price,
        show_staff_price: settings.show_staff_price,
        show_guest_price: settings.show_guest_price,
        hide_expensive_student_meals: settings.hide_expensive_student_meals,
        show_allergens: settings.show_allergens,
        highlight_gluten_free: settings.highlight_gluten_free,
        highlight_veg: settings.highlight_veg,
        highlight_lactose_free: settings.highlight_lactose_free,
    }
}

fn cache_signatures(settings: &Settings) -> Vec<RestaurantCacheSignature> {
    let mut signatures = Vec::new();
    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let mtime_ms =
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
                .unwrap_or(-1);
        signatures.push(RestaurantCacheSignature {
            code: restaurant.code.to_string(),
            mtime_ms,
        });
    }
    signatures
}

fn cached_line_budget(
    key: &PopupLineBudgetKey,
    signatures: &[RestaurantCacheSignature],
) -> Option<Option<usize>> {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.key == *key && entry.signatures == signatures {
        Some(entry.max_lines)
    } else {
        None
    }
}

fn update_line_budget_cache(
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
    max_lines: Option<usize>,
) {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(PopupLineBudgetCache {
            key,
            signatures,
            max_lines,
        });
    }
}

fn max_today_cached_line_count(state: &AppState, today_key: &str) -> Option<usize> {
    let settings = &state.settings;
    let mut max_lines: Option<usize> = None;

    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let raw = match cache::read_cache(restaurant.provider, restaurant.code, &settings.language)
        {
            Some(payload) => payload,
            None => continue,
        };

        let parsed = match api::parse_cached_payload(
            &raw,
            restaurant.provider,
            restaurant,
            &settings.language,
        ) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if !parsed.ok || !is_today_valid_cache(&parsed, restaurant, settings, today_key) {
            continue;
        }

        let candidate_state =
            popup_state_from_cached_result(settings, restaurant, &parsed, today_key);
        let candidate_lines = build_lines(&candidate_state).len();
        max_lines = Some(max_lines.map_or(candidate_lines, |prev| prev.max(candidate_lines)));
    }

    max_lines
}

fn is_today_valid_cache(
    parsed: &api::FetchOutput,
    restaurant: Restaurant,
    settings: &Settings,
    today_key: &str,
) -> bool {
    match restaurant.provider {
        Provider::Antell => cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
            .and_then(date_key_from_epoch_ms)
            .is_some_and(|date| date == today_key),
        _ => !parsed.payload_date.is_empty() && parsed.payload_date == today_key,
    }
}

fn popup_state_from_cached_result(
    settings: &Settings,
    restaurant: Restaurant,
    parsed: &api::FetchOutput,
    today_key: &str,
) -> AppState {
    let restaurant_name = if parsed.restaurant_name.is_empty() {
        restaurant.name.to_string()
    } else {
        parsed.restaurant_name.clone()
    };

    AppState {
        settings: settings.clone(),
        status: if parsed.ok {
            FetchStatus::Ok
        } else {
            FetchStatus::Error
        },
        loading_started_epoch_ms: 0,
        error_message: parsed.error_message.clone(),
        stale_network_error: false,
        today_menu: parsed.today_menu.clone(),
        restaurant_name,
        restaurant_url: parsed.restaurant_url.clone(),
        raw_payload: String::new(),
        provider: restaurant.provider,
        payload_date: parsed.payload_date.clone(),
        stale_date: !parsed.payload_date.is_empty() && parsed.payload_date != today_key,
    }
}

fn local_today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn date_key_from_epoch_ms(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }

    let secs = ms / 1000;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    let mut dt = OffsetDateTime::from_unix_timestamp(secs).ok()?;
    dt = dt.replace_nanosecond(nanos).ok()?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local = dt.to_offset(offset);
    let date = local.date();
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    ))
}

fn position_near_point(width: i32, height: i32, point: POINT) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO::default();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut work_area = RECT::default();
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work_area = info.rcWork;
        }

        let mut x = point.x - width;
        let mut y = point.y - height;
        if x < work_area.left {
            x = work_area.left;
        }
        if y < work_area.top {
            y = work_area.top;
        }
        if x + width > work_area.right {
            x = work_area.right - width;
        }
        if y + height > work_area.bottom {
            y = work_area.bottom - height;
        }

        (x, y)
    }
}

fn position_near_tray_rect(width: i32, height: i32, tray_rect: RECT) -> (i32, i32) {
    unsafe {
        let center = POINT {
            x: (tray_rect.left + tray_rect.right) / 2,
            y: (tray_rect.top + tray_rect.bottom) / 2,
        };
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO::default();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut work_area = RECT::default();
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work_area = info.rcWork;
        }

        let mut x = tray_rect.right - width;
        let mut y = tray_rect.top - height - ANCHOR_GAP;

        if y < work_area.top {
            y = tray_rect.bottom + ANCHOR_GAP;
        }
        if y + height > work_area.bottom {
            y = (tray_rect.top - height - ANCHOR_GAP).max(work_area.top);
        }

        if x < work_area.left {
            x = work_area.left;
        }
        if x + width > work_area.right {
            x = work_area.right - width;
        }
        if y < work_area.top {
            y = work_area.top;
        }
        if y + height > work_area.bottom {
            y = work_area.bottom - height;
        }

        (x, y)
    }
}

fn append_menus(
    lines: &mut Vec<Line>,
    menu: &TodayMenu,
    provider: Provider,
    show_prices: bool,
    price_groups: PriceGroups,
    show_allergens: bool,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
    hide_expensive_student_meals: bool,
) {
    for group in &menu.menus {
        if provider == Provider::Compass && hide_expensive_student_meals {
            if let Some(price) = student_price_eur(&group.price) {
                if price > 4.0 {
                    continue;
                }
            }
        }

        let heading = menu_heading(group, provider, show_prices, price_groups);
        lines.push(Line::Heading(heading));
        for component in &group.components {
            let component = normalize_text(component);
            if component.is_empty() {
                continue;
            }
            let (main, suffix) = split_component_suffix(&component);
            let main_text = if main.is_empty() {
                component.clone()
            } else {
                main
            };
            if !show_allergens {
                lines.push(Line::Text(format!("▸ {}", main_text)));
            } else if !suffix.is_empty() {
                let segments = build_suffix_segments(
                    &suffix,
                    highlight_gluten_free,
                    highlight_veg,
                    highlight_lactose_free,
                );
                lines.push(Line::TextWithSuffixSegments {
                    main: format!("▸ {}", main_text),
                    segments,
                });
            } else {
                lines.push(Line::Text(format!("▸ {}", main_text)));
            }
        }
    }
}

fn build_suffix_segments(
    suffix: &str,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
) -> Vec<(String, bool)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut token_mode = false;

    let mut push_token = |token: &str, out: &mut Vec<(String, bool)>| {
        if token.is_empty() {
            return;
        }
        let upper = token.to_uppercase();
        let highlight = (upper == "G" && highlight_gluten_free)
            || (upper == "VEG" && highlight_veg)
            || (upper == "L" && highlight_lactose_free);
        out.push((token.to_string(), highlight));
    };

    for ch in suffix.chars() {
        if ch.is_alphabetic() {
            if !token_mode {
                if !current.is_empty() {
                    segments.push((current.clone(), false));
                    current.clear();
                }
                token_mode = true;
            }
            current.push(ch);
        } else {
            if token_mode {
                push_token(&current, &mut segments);
                current.clear();
                token_mode = false;
            }
            current.push(ch);
        }
    }

    if !current.is_empty() {
        if token_mode {
            push_token(&current, &mut segments);
        } else {
            segments.push((current, false));
        }
    }

    segments
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn point_in_rect(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn theme_colors(theme: &str) -> (COLORREF, COLORREF, COLORREF, COLORREF, COLORREF, COLORREF) {
    match theme {
        "light" => (
            COLORREF(0x00FFFFFF),
            COLORREF(0x00000000),
            COLORREF(0x00808080),
            COLORREF(0x00F3F3F3),
            COLORREF(0x00DDDDDD),
            COLORREF(0x00C9C9C9),
        ),
        "blue" => (
            COLORREF(0x00562401),
            COLORREF(0x00FFFFFF),
            COLORREF(0x00E7C7A7),
            COLORREF(0x00733809),
            COLORREF(0x00804A1A),
            COLORREF(0x00834D1F),
        ),
        "green" => (
            COLORREF(0x00000000),
            COLORREF(0x0000D000),
            COLORREF(0x00009000),
            COLORREF(0x000B1A0B),
            COLORREF(0x00142D14),
            COLORREF(0x00142D14),
        ),
        _ => (
            COLORREF(0x00000000),
            COLORREF(0x00FFFFFF),
            COLORREF(0x00B0B0B0),
            COLORREF(0x00101010),
            COLORREF(0x00202020),
            COLORREF(0x00202020),
        ),
    }
}

fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

#[allow(non_snake_case)]
fn MulDiv(n_number: i32, n_numerator: i32, n_denominator: i32) -> i32 {
    ((n_number as i64 * n_numerator as i64) / n_denominator as i64) as i32
}
