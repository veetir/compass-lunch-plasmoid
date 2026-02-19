use crate::app::{AppState, FetchStatus};
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, split_component_suffix, text_for,
};
use crate::model::TodayMenu;
use crate::util::to_wstring;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetDeviceCaps,
    GetMonitorInfoW, GetSysColorBrush, GetTextExtentPoint32W, GetTextMetricsW, InvalidateRect,
    MonitorFromPoint, SelectObject, SetBkMode, SetTextColor, TextOutW, COLOR_WINDOW, HDC, HFONT,
    LOGPIXELSY, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, SetWindowPos, ShowWindow, HWND_TOPMOST, SW_HIDE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;

#[derive(Debug, Clone)]
enum Line {
    Heading(String),
    Text(String),
    TextWithSuffix { main: String, suffix: String },
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
        let (text_color, suffix_color) = if state.settings.dark_mode {
            (COLORREF(0x00FFFFFF), COLORREF(0x00B0B0B0))
        } else {
            (COLORREF(0x00000000), COLORREF(0x00808080))
        };
        if state.settings.dark_mode {
            let brush = CreateSolidBrush(COLORREF(0x00000000));
            FillRect(hdc, &rect, brush);
            DeleteObject(brush);
        } else {
            let brush = GetSysColorBrush(COLOR_WINDOW);
            FillRect(hdc, &rect, brush);
        }
        SetBkMode(hdc, TRANSPARENT);

        let (normal_font, bold_font, small_font) = create_fonts(hdc);
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
                Line::TextWithSuffix { main, suffix } => {
                    SelectObject(hdc, normal_font);
                    SetTextColor(hdc, text_color);
                    let main_width = text_width(hdc, &main);
                    draw_text_line(hdc, &main, PADDING_X, y);
                    if !suffix.is_empty() {
                        SelectObject(hdc, small_font);
                        SetTextColor(hdc, suffix_color);
                        draw_text_line(hdc, &suffix, PADDING_X + main_width + 4, y + 1);
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
        EndPaint(hwnd, &ps);
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
        let (normal_font, bold_font, small_font) = create_fonts(hdc);
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
                Line::TextWithSuffix { main, suffix } => {
                    SelectObject(hdc, normal_font);
                    let main_width = text_width(hdc, main);
                    SelectObject(hdc, small_font);
                    let suffix_width = if suffix.is_empty() { 0 } else { text_width(hdc, suffix) + 4 };
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
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        (width + PADDING_X * 2, height)
    }
}

fn create_fonts(hdc: HDC) -> (HFONT, HFONT, HFONT) {
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
        (normal, bold, small)
    }
}

fn build_lines(state: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();

    let restaurant = normalize_text(&state.restaurant_name);
    if !restaurant.is_empty() {
        lines.push(Line::Heading(restaurant));
    }

    if state.today_menu.is_none() && state.status == FetchStatus::Loading {
        lines.push(Line::Text(text_for(&state.settings.language, "loading")));
    }

    let date_line = date_and_time_line(state.today_menu.as_ref(), &state.settings.language);
    if !date_line.is_empty() {
        lines.push(Line::Heading(date_line));
    }

    match &state.today_menu {
        Some(menu) => {
            if !menu.menus.is_empty() {
                append_menus(
                    &mut lines,
                    menu,
                    state.settings.show_prices,
                    state.settings.hide_allergens,
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
        lines.push(Line::Text(text_for(&state.settings.language, "stale")));
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

fn append_menus(lines: &mut Vec<Line>, menu: &TodayMenu, show_prices: bool, hide_allergens: bool) {
    for group in &menu.menus {
        let heading = menu_heading(group, show_prices);
        lines.push(Line::Heading(heading));
        for component in &group.components {
            let component = normalize_text(component);
            if component.is_empty() {
                continue;
            }
            let (main, suffix) = split_component_suffix(&component);
            if hide_allergens {
                let value = if main.is_empty() { component } else { main };
                lines.push(Line::Text(format!("▸ {}", value)));
            } else if !suffix.is_empty() {
                lines.push(Line::TextWithSuffix {
                    main: format!("▸ {}", main),
                    suffix,
                });
            } else {
                lines.push(Line::Text(format!("▸ {}", component)));
            }
        }
    }
}

fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

#[allow(non_snake_case)]
fn MulDiv(n_number: i32, n_numerator: i32, n_denominator: i32) -> i32 {
    ((n_number as i64 * n_numerator as i64) / n_denominator as i64) as i32
}
