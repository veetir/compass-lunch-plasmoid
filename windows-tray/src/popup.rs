use crate::app::{AppState, FetchStatus};
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, split_component_suffix, student_price_eur,
    text_for, PriceGroups,
};
use crate::model::TodayMenu;
use crate::restaurant::Provider;
use crate::util::to_wstring;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetDeviceCaps,
    GetMonitorInfoW, GetTextExtentPoint32W, GetTextMetricsW, InvalidateRect, MonitorFromPoint,
    SelectObject, SetBkMode, SetTextColor, TextOutW, HDC, HFONT, LOGPIXELSY, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, SetWindowPos, ShowWindow, HWND_TOPMOST, SW_HIDE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 10;
const LOADING_HINT_DELAY_MS: i64 = 250;

#[derive(Debug, Clone)]
enum Line {
    Heading(String),
    Text(String),
    TextWithSuffixSegments { main: String, segments: Vec<(String, bool)> },
    Spacer,
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
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_tray_rect(width, height, tray_rect);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
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
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        InvalidateRect(hwnd, None, true);
    }
}

pub fn hide_popup(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
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
        let (bg_color, text_color, suffix_color) = theme_colors(&state.settings.theme);
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        SetBkMode(hdc, TRANSPARENT);

        let (normal_font, bold_font, small_font, small_bold_font) = create_fonts(hdc);
        let _old_font = SelectObject(hdc, normal_font);

        let lines = build_lines(state);
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;

        let mut y = PADDING_Y;
        for line in lines {
            match line {
                Line::Heading(text) => {
                    SelectObject(hdc, bold_font);
                    SetTextColor(hdc, text_color);
                    draw_text_line(hdc, &text, PADDING_X, y);
                    y += line_height;
                }
                Line::Text(text) => {
                    SelectObject(hdc, normal_font);
                    SetTextColor(hdc, text_color);
                    draw_text_line(hdc, &text, PADDING_X, y);
                    y += line_height;
                }
                Line::TextWithSuffixSegments { main, segments } => {
                    SelectObject(hdc, normal_font);
                    SetTextColor(hdc, text_color);
                    let main_width = text_width(hdc, &main);
                    draw_text_line(hdc, &main, PADDING_X, y);
                    if !segments.is_empty() {
                        draw_text_segments(
                            hdc,
                            &segments,
                            PADDING_X + main_width + 4,
                            y + 1,
                            small_font,
                            small_bold_font,
                            suffix_color,
                        );
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
        let lines = build_lines(state);
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;

        let mut width = 240;
        for line in &lines {
            let w = match line {
                Line::Heading(text) => {
                    SelectObject(hdc, bold_font);
                    text_width(hdc, text)
                }
                Line::Text(text) => {
                    SelectObject(hdc, normal_font);
                    text_width(hdc, text)
                }
                Line::TextWithSuffixSegments { main, segments } => {
                    SelectObject(hdc, normal_font);
                    let main_width = text_width(hdc, main);
                    let mut suffix_width = 0;
                    for (segment, bold) in segments {
                        let font = if *bold { small_bold_font } else { small_font };
                        SelectObject(hdc, font);
                        suffix_width += text_width(hdc, segment);
                    }
                    if suffix_width > 0 {
                        suffix_width += 4;
                    }
                    main_width + suffix_width
                }
                Line::Spacer => 0,
            };
            if w > width {
                width = w;
            }
        }

        let height = (lines.len() as i32 * line_height) + PADDING_Y * 2;
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        (width + PADDING_X * 2, height)
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
            let main_text = if main.is_empty() { component.clone() } else { main };
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

fn theme_colors(theme: &str) -> (COLORREF, COLORREF, COLORREF) {
    match theme {
        "light" => (
            COLORREF(0x00FFFFFF),
            COLORREF(0x00000000),
            COLORREF(0x00808080),
        ),
        "blue" => (
            COLORREF(0x00562401),
            COLORREF(0x00FFFFFF),
            COLORREF(0x00E7C7A7),
        ),
        "green" => (
            COLORREF(0x00000000),
            COLORREF(0x0000D000),
            COLORREF(0x00009000),
        ),
        _ => (
            COLORREF(0x00000000),
            COLORREF(0x00FFFFFF),
            COLORREF(0x00B0B0B0),
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
