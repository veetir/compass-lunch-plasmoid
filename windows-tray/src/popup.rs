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
    GetClientRect, GetCursorPos, GetWindowRect, KillTimer, SetTimer, SetWindowPos, ShowWindow,
    HWND_TOPMOST, SWP_SHOWWINDOW, SW_HIDE,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 10;
const POPUP_MAX_WIDTH: i32 = 525;
const POPUP_MIN_WIDTH: i32 = 320;
const POPUP_MAX_CONTENT_WIDTH: i32 = POPUP_MAX_WIDTH - PADDING_X * 2;
const POPUP_MIN_CONTENT_WIDTH: i32 = POPUP_MIN_WIDTH - PADDING_X * 2;
const HEADER_HEIGHT: i32 = 46;
const HEADER_BUTTON_SIZE: i32 = 30;
const HEADER_BUTTON_GAP: i32 = 8;
const LOADING_HINT_DELAY_MS: i64 = 250;
const MAX_DYNAMIC_LINES: usize = 35;
const POPUP_ANIM_INTERVAL_MS: u32 = 33;
const POPUP_OPEN_ANIM_MS: i64 = 120;
const POPUP_CLOSE_ANIM_MS: i64 = 90;
const POPUP_SWITCH_ANIM_MS: i64 = 120;
const POPUP_SWITCH_OFFSET_PX: i32 = 6;

static POPUP_LINE_BUDGET_CACHE: OnceLock<Mutex<Option<PopupLineBudgetCache>>> = OnceLock::new();
static POPUP_ANIMATION: OnceLock<Mutex<Option<PopupAnimation>>> = OnceLock::new();

pub const POPUP_ANIM_TIMER_ID: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupLineBudgetKey {
    today_key: String,
    language: String,
    theme: String,
    dpi_y: i32,
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
    max_wrapped_lines: Option<usize>,
    max_content_width_px: Option<i32>,
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

#[derive(Debug, Clone)]
enum PopupAnimationKind {
    Open {
        lines: Vec<Line>,
        title: String,
    },
    Close {
        lines: Vec<Line>,
        title: String,
    },
    Switch {
        old_lines: Vec<Line>,
        new_lines: Vec<Line>,
        old_title: String,
        new_title: String,
        direction: i32,
    },
}

#[derive(Debug, Clone)]
struct PopupAnimation {
    hwnd: HWND,
    start_epoch_ms: i64,
    duration_ms: i64,
    kind: PopupAnimationKind,
}

#[derive(Debug, Clone)]
enum PopupAnimationFrame {
    Open {
        lines: Vec<Line>,
        title: String,
        progress: f32,
    },
    Close {
        lines: Vec<Line>,
        title: String,
        progress: f32,
    },
    Switch {
        old_lines: Vec<Line>,
        new_lines: Vec<Line>,
        old_title: String,
        new_title: String,
        direction: i32,
        progress: f32,
    },
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
    if is_visible(hwnd) {
        begin_close_animation(hwnd, state);
    } else {
        show_popup(hwnd, state);
    }
}

pub fn show_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let (x, y) = position_near_point(width, height, cursor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_tray_rect(width, height, tray_rect);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
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
        clear_animation_state(hwnd);
        let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

fn begin_open_animation(hwnd: HWND, state: &AppState) {
    start_animation(
        hwnd,
        POPUP_OPEN_ANIM_MS,
        PopupAnimationKind::Open {
            lines: build_lines(state),
            title: header_title(state),
        },
    );
}

fn start_animation(hwnd: HWND, duration_ms: i64, kind: PopupAnimationKind) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        *guard = Some(PopupAnimation {
            hwnd,
            start_epoch_ms: now_epoch_ms(),
            duration_ms: duration_ms.max(1),
            kind,
        });
    }
    unsafe {
        let _ = SetTimer(hwnd, POPUP_ANIM_TIMER_ID, POPUP_ANIM_INTERVAL_MS, None);
        InvalidateRect(hwnd, None, true);
    }
}

fn clear_animation_state(hwnd: HWND) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if guard.as_ref().is_some_and(|anim| anim.hwnd == hwnd) {
            *guard = None;
        }
    }
}

fn current_animation_frame(hwnd: HWND) -> Option<PopupAnimationFrame> {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    let guard = store.lock().ok()?;
    let anim = guard.as_ref()?;
    if anim.hwnd != hwnd {
        return None;
    }
    let elapsed = now_epoch_ms().saturating_sub(anim.start_epoch_ms);
    let progress = (elapsed as f32 / anim.duration_ms.max(1) as f32).clamp(0.0, 1.0);
    match &anim.kind {
        PopupAnimationKind::Open { lines, title } => Some(PopupAnimationFrame::Open {
            lines: lines.clone(),
            title: title.clone(),
            progress,
        }),
        PopupAnimationKind::Close { lines, title } => Some(PopupAnimationFrame::Close {
            lines: lines.clone(),
            title: title.clone(),
            progress,
        }),
        PopupAnimationKind::Switch {
            old_lines,
            new_lines,
            old_title,
            new_title,
            direction,
        } => Some(PopupAnimationFrame::Switch {
            old_lines: old_lines.clone(),
            new_lines: new_lines.clone(),
            old_title: old_title.clone(),
            new_title: new_title.clone(),
            direction: *direction,
            progress,
        }),
    }
}

pub fn begin_close_animation(hwnd: HWND, state: &AppState) {
    if !is_visible(hwnd) {
        return;
    }
    start_animation(
        hwnd,
        POPUP_CLOSE_ANIM_MS,
        PopupAnimationKind::Close {
            lines: build_lines(state),
            title: header_title(state),
        },
    );
}

pub fn begin_switch_animation(
    hwnd: HWND,
    old_state: &AppState,
    new_state: &AppState,
    direction: i32,
) {
    start_animation(
        hwnd,
        POPUP_SWITCH_ANIM_MS,
        PopupAnimationKind::Switch {
            old_lines: build_lines(old_state),
            new_lines: build_lines(new_state),
            old_title: header_title(old_state),
            new_title: header_title(new_state),
            direction,
        },
    );
}

pub fn tick_animation(hwnd: HWND) {
    let now = now_epoch_ms();
    let mut active = false;
    let mut finished = false;
    let mut hide_after = false;

    {
        let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
        let mut guard = match store.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        if let Some(anim) = guard.as_ref() {
            if anim.hwnd == hwnd {
                active = true;
                let elapsed = now.saturating_sub(anim.start_epoch_ms);
                if elapsed >= anim.duration_ms.max(1) {
                    finished = true;
                    hide_after = matches!(anim.kind, PopupAnimationKind::Close { .. });
                }
            }
        }
        if finished {
            *guard = None;
        }
    }

    unsafe {
        if !active {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            return;
        }
        if finished {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            if hide_after {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
        }
        InvalidateRect(hwnd, None, true);
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
        let palette = theme_palette(&state.settings.theme);
        let brush = CreateSolidBrush(palette.bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        SetBkMode(hdc, TRANSPARENT);

        let (normal_font, bold_font, small_font, small_bold_font) =
            create_fonts(hdc, &state.settings.theme);
        let _old_font = SelectObject(hdc, normal_font);

        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;
        let content_width = (width - PADDING_X * 2).max(40);
        let animation = current_animation_frame(hwnd);

        let header_rect = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.top + HEADER_HEIGHT,
        };
        let header_brush = CreateSolidBrush(palette.header_bg_color);
        FillRect(hdc, &header_rect, header_brush);
        DeleteObject(header_brush);

        let layout = header_layout(width);
        draw_header_button(
            hdc,
            &layout.prev,
            "<",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
        );
        draw_header_button(
            hdc,
            &layout.next,
            ">",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
        );
        draw_header_button(
            hdc,
            &layout.close,
            "X",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
        );

        let divider_rect = RECT {
            left: rect.left,
            top: header_rect.bottom - 1,
            right: rect.right,
            bottom: header_rect.bottom,
        };
        let divider_brush = CreateSolidBrush(palette.divider_color);
        FillRect(hdc, &divider_rect, divider_brush);
        DeleteObject(divider_brush);

        if let Some(frame) = animation {
            match frame {
                PopupAnimationFrame::Open {
                    lines,
                    title,
                    progress,
                } => {
                    let y_offset =
                        ((1.0 - progress) * POPUP_SWITCH_OFFSET_PX as f32).round() as i32;
                    let layer_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, progress);
                    let layer_heading =
                        lerp_color(palette.bg_color, palette.heading_color, progress);
                    let layer_title =
                        lerp_color(palette.bg_color, palette.header_title_color, progress);
                    let layer_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                    let layer_suffix_highlight =
                        lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                    draw_content_layer(
                        hdc,
                        &title,
                        &lines,
                        DrawLayerParams {
                            width,
                            content_width,
                            body_text_color: layer_body_text,
                            heading_color: layer_heading,
                            header_title_color: layer_title,
                            suffix_color: layer_suffix,
                            suffix_highlight_color: layer_suffix_highlight,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            y_offset,
                        },
                    );
                }
                PopupAnimationFrame::Close {
                    lines,
                    title,
                    progress,
                } => {
                    let y_offset = -((progress * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                    let layer_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                    let layer_heading =
                        lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                    let layer_title =
                        lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                    let layer_suffix =
                        lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                    let layer_suffix_highlight = lerp_color(
                        palette.bg_color,
                        palette.suffix_highlight_color,
                        1.0 - progress,
                    );
                    draw_content_layer(
                        hdc,
                        &title,
                        &lines,
                        DrawLayerParams {
                            width,
                            content_width,
                            body_text_color: layer_body_text,
                            heading_color: layer_heading,
                            header_title_color: layer_title,
                            suffix_color: layer_suffix,
                            suffix_highlight_color: layer_suffix_highlight,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            y_offset,
                        },
                    );
                }
                PopupAnimationFrame::Switch {
                    old_lines,
                    new_lines,
                    old_title,
                    new_title,
                    direction,
                    progress,
                } => {
                    let dir = if direction >= 0 { 1 } else { -1 };
                    let old_offset =
                        -dir * ((progress * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                    let new_offset =
                        dir * (((1.0 - progress) * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                    let old_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                    let old_heading =
                        lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                    let old_title_color =
                        lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                    let old_suffix =
                        lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                    let old_suffix_highlight = lerp_color(
                        palette.bg_color,
                        palette.suffix_highlight_color,
                        1.0 - progress,
                    );
                    let new_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, progress);
                    let new_heading = lerp_color(palette.bg_color, palette.heading_color, progress);
                    let new_title_color =
                        lerp_color(palette.bg_color, palette.header_title_color, progress);
                    let new_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                    let new_suffix_highlight =
                        lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                    draw_content_layer(
                        hdc,
                        &old_title,
                        &old_lines,
                        DrawLayerParams {
                            width,
                            content_width,
                            body_text_color: old_body_text,
                            heading_color: old_heading,
                            header_title_color: old_title_color,
                            suffix_color: old_suffix,
                            suffix_highlight_color: old_suffix_highlight,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            y_offset: old_offset,
                        },
                    );
                    draw_content_layer(
                        hdc,
                        &new_title,
                        &new_lines,
                        DrawLayerParams {
                            width,
                            content_width,
                            body_text_color: new_body_text,
                            heading_color: new_heading,
                            header_title_color: new_title_color,
                            suffix_color: new_suffix,
                            suffix_highlight_color: new_suffix_highlight,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            y_offset: new_offset,
                        },
                    );
                }
            }
        } else {
            let lines = build_lines(state);
            let title = header_title(state);
            draw_content_layer(
                hdc,
                &title,
                &lines,
                DrawLayerParams {
                    width,
                    content_width,
                    body_text_color: palette.body_text_color,
                    heading_color: palette.heading_color,
                    header_title_color: palette.header_title_color,
                    suffix_color: palette.suffix_color,
                    suffix_highlight_color: palette.suffix_highlight_color,
                    layout: &layout,
                    metrics: &metrics,
                    line_height,
                    normal_font,
                    bold_font,
                    small_font,
                    small_bold_font,
                    y_offset: 0,
                },
            );
        }

        SelectObject(hdc, _old_font);
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        EndPaint(hwnd, &ps);
    }
}

struct DrawLayerParams<'a> {
    width: i32,
    content_width: i32,
    body_text_color: COLORREF,
    heading_color: COLORREF,
    header_title_color: COLORREF,
    suffix_color: COLORREF,
    suffix_highlight_color: COLORREF,
    layout: &'a HeaderLayout,
    metrics: &'a TEXTMETRICW,
    line_height: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    y_offset: i32,
}

fn draw_content_layer(hdc: HDC, title: &str, lines: &[Line], params: DrawLayerParams<'_>) {
    unsafe {
        SelectObject(hdc, params.bold_font);
        SetTextColor(hdc, params.header_title_color);
    }

    let clipped_title = fit_text_to_width(
        hdc,
        title,
        (params.layout.close.left - params.layout.next.right - 24).max(40),
    );
    let title_width = text_width(hdc, &clipped_title);
    let title_x = ((params.width - title_width) / 2).max(params.layout.next.right + 12);
    let title_y = ((HEADER_HEIGHT - params.metrics.tmHeight as i32) / 2 - 1) + params.y_offset;
    draw_text_line(hdc, &clipped_title, title_x, title_y);

    let mut y = HEADER_HEIGHT + PADDING_Y + params.y_offset;
    for line in lines {
        match line {
            Line::Heading(text) => {
                unsafe {
                    SelectObject(hdc, params.bold_font);
                    SetTextColor(hdc, params.heading_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, PADDING_X, y);
                        y += params.line_height;
                    }
                }
            }
            Line::Text(text) => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, PADDING_X, y);
                        y += params.line_height;
                    }
                }
            }
            Line::TextWithSuffixSegments { main, segments } => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let styled_width = text_with_suffix_width(
                    hdc,
                    params.normal_font,
                    params.small_font,
                    params.small_bold_font,
                    main,
                    segments,
                );
                if styled_width <= params.content_width {
                    let mut suffix_width = 0;
                    for (segment, bold) in segments {
                        let font = if *bold {
                            params.small_bold_font
                        } else {
                            params.small_font
                        };
                        unsafe {
                            SelectObject(hdc, font);
                        }
                        suffix_width += text_width(hdc, segment);
                    }
                    let max_main = (params.content_width - suffix_width - 4).max(24);
                    unsafe {
                        SelectObject(hdc, params.normal_font);
                    }
                    let clipped_main = fit_text_to_width(hdc, main, max_main);
                    let main_width = text_width(hdc, &clipped_main);
                    draw_text_line(hdc, &clipped_main, PADDING_X, y);
                    if !segments.is_empty() {
                        let suffix_x = PADDING_X + main_width + 4;
                        if suffix_x < (PADDING_X + params.content_width) {
                            draw_text_segments(
                                hdc,
                                segments,
                                suffix_x,
                                y + 1,
                                params.small_font,
                                params.small_bold_font,
                                params.suffix_color,
                                params.suffix_highlight_color,
                            );
                        }
                    }
                    y += params.line_height;
                    continue;
                }
                unsafe {
                    SelectObject(hdc, params.normal_font);
                }
                let plain = flatten_text_with_suffix(main, segments);
                let wrapped = wrap_text_to_width(hdc, &plain, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, PADDING_X, y);
                        y += params.line_height;
                    }
                }
            }
            Line::Spacer => {
                y += params.line_height / 2;
            }
        }
    }
}

fn measure_lines_layout(
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    lines: &[Line],
    wrap_content_width: i32,
) -> LineLayoutMetrics {
    let wrap_width = wrap_content_width.max(40);
    let mut required_content_width = 0;
    let mut wrapped_line_count = 0usize;

    for line in lines {
        match line {
            Line::Heading(text) => {
                let width = text_width_with_font(hdc, bold_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, bold_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::Text(text) => {
                let width = text_width_with_font(hdc, normal_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, normal_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::TextWithSuffixSegments { main, segments } => {
                let styled_width = text_with_suffix_width(
                    hdc,
                    normal_font,
                    small_font,
                    small_bold_font,
                    main,
                    segments,
                );
                required_content_width = required_content_width.max(styled_width);
                if styled_width <= wrap_width {
                    wrapped_line_count += 1;
                } else {
                    let plain = flatten_text_with_suffix(main, segments);
                    let rows =
                        wrapped_line_count_for_text(hdc, normal_font, &plain, wrap_width).max(1);
                    wrapped_line_count += rows;
                }
            }
            Line::Spacer => {
                wrapped_line_count += 1;
            }
        }
    }

    LineLayoutMetrics {
        required_content_width,
        wrapped_line_count,
    }
}

fn wrapped_line_count_for_text(hdc: HDC, font: HFONT, text: &str, max_width: i32) -> usize {
    let wrapped = wrap_text_to_width_with_font(hdc, font, text, max_width);
    wrapped.len()
}

fn wrap_text_to_width_with_font(hdc: HDC, font: HFONT, text: &str, max_width: i32) -> Vec<String> {
    unsafe {
        let old = SelectObject(hdc, font);
        let wrapped = wrap_text_to_width(hdc, text, max_width);
        SelectObject(hdc, old);
        wrapped
    }
}

fn wrap_text_to_width(hdc: HDC, text: &str, max_width: i32) -> Vec<String> {
    let clean = normalize_text(text);
    if clean.is_empty() {
        return Vec::new();
    }
    let limit = max_width.max(16);
    if text_width(hdc, &clean) <= limit {
        return vec![clean];
    }

    let words: Vec<String> = clean
        .split_whitespace()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();
    if words.is_empty() {
        return vec![clean];
    }

    let mut rows: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in words {
        let candidate = if current.is_empty() {
            word.clone()
        } else {
            format!("{} {}", current, word)
        };
        if text_width(hdc, &candidate) <= limit {
            current = candidate;
            continue;
        }

        if !current.is_empty() {
            rows.push(current.clone());
            current.clear();
        }

        if text_width(hdc, &word) <= limit {
            current = word;
        } else {
            rows.extend(split_long_token_to_width(hdc, &word, limit));
        }
    }

    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(clean);
    }
    rows
}

fn split_long_token_to_width(hdc: HDC, token: &str, max_width: i32) -> Vec<String> {
    let mut rows = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        let mut candidate = current.clone();
        candidate.push(ch);
        if !current.is_empty() && text_width(hdc, &candidate) > max_width {
            rows.push(current.clone());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(token.to_string());
    }
    rows
}

fn text_width_with_font(hdc: HDC, font: HFONT, text: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let width = text_width(hdc, text);
        SelectObject(hdc, old);
        width
    }
}

fn text_with_suffix_width(
    hdc: HDC,
    normal_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    main: &str,
    segments: &[(String, bool)],
) -> i32 {
    let main_width = text_width_with_font(hdc, normal_font, main);
    if segments.is_empty() {
        return main_width;
    }

    let mut suffix_width = 0;
    for (segment, bold) in segments {
        let font = if *bold { small_bold_font } else { small_font };
        suffix_width += text_width_with_font(hdc, font, segment);
    }
    main_width + suffix_width + 4
}

fn flatten_text_with_suffix(main: &str, segments: &[(String, bool)]) -> String {
    let mut out = normalize_text(main);
    for (segment, _) in segments {
        out.push_str(segment);
    }
    out
}

fn draw_text_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    x: i32,
    y: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    normal_color: COLORREF,
    highlight_color: COLORREF,
) {
    let mut cursor = x;
    for (text, bold) in segments {
        let font = if *bold { bold_font } else { normal_font };
        let color = if *bold { highlight_color } else { normal_color };
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
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        let (normal_font, bold_font, small_font, small_bold_font) =
            create_fonts(hdc, &state.settings.theme);
        let current_lines = build_lines(state);
        let current_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &current_lines,
            POPUP_MAX_CONTENT_WIDTH,
        );
        let budget = popup_cached_layout_budget(
            state,
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            dpi_y,
        );
        let target_content_width = budget
            .max_content_width_px
            .unwrap_or(current_metrics.required_content_width)
            .clamp(POPUP_MIN_CONTENT_WIDTH, POPUP_MAX_CONTENT_WIDTH);
        let current_wrapped_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &current_lines,
            target_content_width,
        );
        let mut target_lines = budget
            .max_wrapped_lines
            .unwrap_or(current_wrapped_metrics.wrapped_line_count);
        if budget.max_wrapped_lines.is_some() {
            target_lines = target_lines.max(current_wrapped_metrics.wrapped_line_count);
        }
        target_lines = target_lines.min(MAX_DYNAMIC_LINES);
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + LINE_GAP;
        let height = HEADER_HEIGHT + (target_lines as i32 * line_height) + PADDING_Y * 2;
        let width = (target_content_width + PADDING_X * 2).clamp(POPUP_MIN_WIDTH, POPUP_MAX_WIDTH);
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        (width, height.max(HEADER_HEIGHT + 120))
    }
}

fn create_fonts(hdc: HDC, theme: &str) -> (HFONT, HFONT, HFONT, HFONT) {
    unsafe {
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY);
        let height_normal = -MulDiv(12, dpi, 72);
        let height_small = -MulDiv(10, dpi, 72);
        let face = to_wstring(theme_font_family(theme));

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

    if state.stale_date {
        lines.push(Line::Heading("[STALE]".to_string()));
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

#[derive(Debug, Clone, Copy)]
struct CachedLayoutBudget {
    max_wrapped_lines: Option<usize>,
    max_content_width_px: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
struct LineLayoutMetrics {
    required_content_width: i32,
    wrapped_line_count: usize,
}

fn popup_cached_layout_budget(
    state: &AppState,
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    dpi_y: i32,
) -> CachedLayoutBudget {
    let today_key = local_today_key();
    let key = line_budget_key(&state.settings, &today_key, dpi_y);
    let signatures = cache_signatures(&state.settings);
    if let Some(budget) = cached_line_budget(&key, &signatures) {
        return budget;
    }

    let budget = max_today_cached_layout_budget(
        state,
        &today_key,
        hdc,
        normal_font,
        bold_font,
        small_font,
        small_bold_font,
    );
    update_line_budget_cache(key, signatures, budget);
    budget
}

fn line_budget_key(settings: &Settings, today_key: &str, dpi_y: i32) -> PopupLineBudgetKey {
    PopupLineBudgetKey {
        today_key: today_key.to_string(),
        language: settings.language.clone(),
        theme: settings.theme.clone(),
        dpi_y,
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
) -> Option<CachedLayoutBudget> {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.key == *key && entry.signatures == signatures {
        Some(CachedLayoutBudget {
            max_wrapped_lines: entry.max_wrapped_lines,
            max_content_width_px: entry.max_content_width_px,
        })
    } else {
        None
    }
}

fn update_line_budget_cache(
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
    budget: CachedLayoutBudget,
) {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(PopupLineBudgetCache {
            key,
            signatures,
            max_wrapped_lines: budget.max_wrapped_lines,
            max_content_width_px: budget.max_content_width_px,
        });
    }
}

fn max_today_cached_layout_budget(
    state: &AppState,
    today_key: &str,
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
) -> CachedLayoutBudget {
    let settings = &state.settings;
    let mut max_wrapped_lines: Option<usize> = None;
    let mut max_content_width_px: Option<i32> = None;

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
        let candidate_lines = build_lines(&candidate_state);
        let metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &candidate_lines,
            POPUP_MAX_CONTENT_WIDTH,
        );
        max_wrapped_lines = Some(
            max_wrapped_lines.map_or(metrics.wrapped_line_count, |prev| {
                prev.max(metrics.wrapped_line_count)
            }),
        );
        max_content_width_px = Some(
            max_content_width_px.map_or(metrics.required_content_width, |prev| {
                prev.max(metrics.required_content_width)
            }),
        );
    }

    CachedLayoutBudget {
        max_wrapped_lines,
        max_content_width_px,
    }
}

fn is_today_valid_cache(
    parsed: &api::FetchOutput,
    restaurant: Restaurant,
    settings: &Settings,
    today_key: &str,
) -> bool {
    match restaurant.provider {
        Provider::Antell => {
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
                .and_then(date_key_from_epoch_ms)
                .is_some_and(|date| date == today_key)
        }
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

fn lerp_color(from: COLORREF, to: COLORREF, t: f32) -> COLORREF {
    let p = t.clamp(0.0, 1.0);
    let (fr, fg, fb) = color_channels(from);
    let (tr, tg, tb) = color_channels(to);
    let r = fr as f32 + (tr as f32 - fr as f32) * p;
    let g = fg as f32 + (tg as f32 - fg as f32) * p;
    let b = fb as f32 + (tb as f32 - fb as f32) * p;
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn color_channels(color: COLORREF) -> (u8, u8, u8) {
    let value = color.0;
    let r = (value & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = ((value >> 16) & 0xFF) as u8;
    (r, g, b)
}

#[derive(Debug, Clone, Copy)]
struct ThemePalette {
    bg_color: COLORREF,
    body_text_color: COLORREF,
    heading_color: COLORREF,
    header_title_color: COLORREF,
    suffix_color: COLORREF,
    suffix_highlight_color: COLORREF,
    header_bg_color: COLORREF,
    button_bg_color: COLORREF,
    divider_color: COLORREF,
}

fn theme_palette(theme: &str) -> ThemePalette {
    match theme {
        "light" => ThemePalette {
            bg_color: COLORREF(0x00FFFFFF),
            body_text_color: COLORREF(0x00000000),
            heading_color: COLORREF(0x00000000),
            header_title_color: COLORREF(0x00000000),
            suffix_color: COLORREF(0x00808080),
            suffix_highlight_color: COLORREF(0x00808080),
            header_bg_color: COLORREF(0x00F3F3F3),
            button_bg_color: COLORREF(0x00DDDDDD),
            divider_color: COLORREF(0x00C9C9C9),
        },
        "blue" => ThemePalette {
            bg_color: COLORREF(0x00562401),
            body_text_color: COLORREF(0x00FFFFFF),
            heading_color: COLORREF(0x00FFFFFF),
            header_title_color: COLORREF(0x00FFFFFF),
            suffix_color: COLORREF(0x00E7C7A7),
            suffix_highlight_color: COLORREF(0x00E7C7A7),
            header_bg_color: COLORREF(0x00733809),
            button_bg_color: COLORREF(0x00804A1A),
            divider_color: COLORREF(0x00834D1F),
        },
        "green" => ThemePalette {
            bg_color: COLORREF(0x00000000),
            body_text_color: COLORREF(0x0000D000),
            heading_color: COLORREF(0x0000D000),
            header_title_color: COLORREF(0x0000D000),
            suffix_color: COLORREF(0x00009000),
            suffix_highlight_color: COLORREF(0x0000D000),
            header_bg_color: COLORREF(0x000B1A0B),
            button_bg_color: COLORREF(0x00142D14),
            divider_color: COLORREF(0x00142D14),
        },
        "teletext1" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(255, 255, 255),
            heading_color: rgb(0, 255, 255),
            header_title_color: rgb(255, 255, 0),
            suffix_color: rgb(0, 255, 0),
            suffix_highlight_color: rgb(255, 0, 255),
            header_bg_color: rgb(0, 0, 180),
            button_bg_color: rgb(0, 0, 140),
            divider_color: rgb(255, 0, 0),
        },
        "teletext2" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(225, 255, 225),
            heading_color: rgb(255, 0, 255),
            header_title_color: rgb(0, 96, 255),
            suffix_color: rgb(0, 255, 150),
            suffix_highlight_color: rgb(255, 255, 0),
            header_bg_color: rgb(0, 215, 0),
            button_bg_color: rgb(0, 145, 0),
            divider_color: rgb(255, 0, 255),
        },
        _ => ThemePalette {
            bg_color: COLORREF(0x00000000),
            body_text_color: COLORREF(0x00FFFFFF),
            heading_color: COLORREF(0x00FFFFFF),
            header_title_color: COLORREF(0x00FFFFFF),
            suffix_color: COLORREF(0x00B0B0B0),
            suffix_highlight_color: COLORREF(0x00B0B0B0),
            header_bg_color: COLORREF(0x00101010),
            button_bg_color: COLORREF(0x00202020),
            divider_color: COLORREF(0x00202020),
        },
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

fn theme_font_family(theme: &str) -> &'static str {
    match theme {
        "teletext1" | "teletext2" => "Consolas",
        _ => "Segoe UI",
    }
}

fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

#[allow(non_snake_case)]
fn MulDiv(n_number: i32, n_numerator: i32, n_denominator: i32) -> i32 {
    ((n_number as i64 * n_numerator as i64) / n_denominator as i64) as i32
}
