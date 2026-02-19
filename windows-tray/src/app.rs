use crate::api::{self, FetchOutput};
use crate::cache;
use crate::model::TodayMenu;
use crate::settings::{load_settings, save_settings, Settings};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use windows::Win32::Foundation::HWND;

pub const RESTAURANTS: [&str; 3] = ["0437", "0439", "0436"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    Idle,
    Loading,
    Ok,
    Stale,
    Error,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub settings: Settings,
    pub status: FetchStatus,
    pub error_message: String,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub raw_payload: String,
}

#[derive(Default, Clone, Copy)]
struct WindowHandles {
    tray: HWND,
    popup: HWND,
}

pub struct App {
    pub no_tray: bool,
    state: Arc<Mutex<AppState>>,
    hwnds: Mutex<WindowHandles>,
    hover_point: Mutex<Option<(i32, i32)>>,
    context_menu_open: Mutex<bool>,
}

impl App {
    pub fn new(no_tray: bool) -> Self {
        let settings = load_settings();
        let state = AppState {
            settings,
            status: FetchStatus::Idle,
            error_message: String::new(),
            today_menu: None,
            restaurant_name: String::new(),
            restaurant_url: String::new(),
            raw_payload: String::new(),
        };
        Self {
            no_tray,
            state: Arc::new(Mutex::new(state)),
            hwnds: Mutex::new(WindowHandles::default()),
            hover_point: Mutex::new(None),
            context_menu_open: Mutex::new(false),
        }
    }

    pub fn set_hwnds(&self, tray: HWND, popup: HWND) {
        let mut hwnds = self.hwnds.lock().unwrap();
        hwnds.tray = tray;
        hwnds.popup = popup;
    }

    pub fn hwnd_tray(&self) -> HWND {
        self.hwnds.lock().unwrap().tray
    }

    pub fn hwnd_popup(&self) -> HWND {
        self.hwnds.lock().unwrap().popup
    }

    pub fn snapshot(&self) -> AppState {
        self.state.lock().unwrap().clone()
    }

    pub fn load_cache_for_current(&self) {
        let mut state = self.state.lock().unwrap();
        let code = state.settings.restaurant_code.clone();
        let lang = state.settings.language.clone();
        drop(state);
        if let Some(raw) = cache::read_cache(&code, &lang) {
            match api::parse_cached_payload(&raw) {
                Ok(result) => {
                    self.apply_cached_result(result);
                }
                Err(err) => {
                    let mut state = self.state.lock().unwrap();
                    state.status = FetchStatus::Error;
                    state.error_message = err.to_string();
                }
            }
        }
    }

    fn apply_cached_result(&self, result: FetchOutput) {
        let mut state = self.state.lock().unwrap();
        state.raw_payload = result.raw_json;
        state.restaurant_name = result.restaurant_name;
        state.restaurant_url = result.restaurant_url;
        state.today_menu = result.today_menu;
        if result.ok {
            state.status = FetchStatus::Stale;
            state.error_message.clear();
        } else {
            state.status = FetchStatus::Error;
            state.error_message = result.error_message;
        }
    }

    pub fn start_refresh(&self) {
        let hwnd = self.hwnd_tray();
        let settings = {
            let mut state = self.state.lock().unwrap();
            state.status = FetchStatus::Loading;
            state.error_message.clear();
            state.settings.clone()
        };
        std::thread::spawn(move || {
            let result = api::fetch_today(&settings);
            let boxed = Box::new(result);
            let ptr = Box::into_raw(boxed) as isize;
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    crate::winmsg::WM_APP_FETCH_COMPLETE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(ptr),
                );
            }
        });
    }

    pub fn apply_fetch_result(&self, result: FetchOutput) {
        let mut state = self.state.lock().unwrap();
        if result.ok {
            state.status = FetchStatus::Ok;
            state.error_message.clear();
            state.raw_payload = result.raw_json.clone();
            state.restaurant_name = result.restaurant_name;
            state.restaurant_url = result.restaurant_url;
            state.today_menu = result.today_menu;
            state.settings.last_updated_epoch_ms = now_epoch_ms();
            let _ = save_settings(&state.settings);
            let _ = cache::write_cache(
                &state.settings.restaurant_code,
                &state.settings.language,
                &result.raw_json,
            );
        } else {
            if !state.raw_payload.is_empty() {
                state.status = FetchStatus::Stale;
            } else {
                state.status = FetchStatus::Error;
            }
            state.error_message = result.error_message;
        }
    }

    pub fn set_restaurant(&self, code: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.restaurant_code = code.to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.status = FetchStatus::Idle;
    }

    pub fn set_language(&self, language: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.language = language.to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.status = FetchStatus::Idle;
    }

    pub fn toggle_show_prices(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_prices = !state.settings.show_prices;
        let _ = save_settings(&state.settings);
    }

    pub fn set_refresh_minutes(&self, minutes: u32) {
        let mut state = self.state.lock().unwrap();
        state.settings.refresh_minutes = minutes;
        let _ = save_settings(&state.settings);
    }

    pub fn cycle_restaurant(&self, direction: i32) {
        let mut state = self.state.lock().unwrap();
        let current = state.settings.restaurant_code.as_str();
        let mut idx = RESTAURANTS.iter().position(|c| *c == current).unwrap_or(0) as i32;
        idx += direction;
        if idx < 0 {
            idx = RESTAURANTS.len() as i32 - 1;
        } else if idx >= RESTAURANTS.len() as i32 {
            idx = 0;
        }
        state.settings.restaurant_code = RESTAURANTS[idx as usize].to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.status = FetchStatus::Idle;
    }

    pub fn open_current_url(&self) {
        let url = {
            let state = self.state.lock().unwrap();
            state.restaurant_url.clone()
        };
        if url.is_empty() {
            return;
        }
        let wide = crate::util::to_wstring(&url);
        unsafe {
            windows::Win32::UI::Shell::ShellExecuteW(
                None,
                windows::core::PCWSTR(crate::util::to_wstring("open").as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }

    pub fn refresh_minutes(&self) -> u32 {
        let state = self.state.lock().unwrap();
        state.settings.refresh_minutes
    }

    pub fn maybe_refresh_on_selection(&self) {
        let (code, language, refresh_minutes) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.restaurant_code.clone(),
                state.settings.language.clone(),
                state.settings.refresh_minutes,
            )
        };

        if refresh_minutes == 0 {
            return;
        }

        let now = now_epoch_ms();
        let should_fetch = match cache::cache_mtime_ms(&code, &language) {
            None => true,
            Some(ts) => now.saturating_sub(ts) >= (refresh_minutes as i64) * 60_000,
        };

        if should_fetch {
            self.start_refresh();
        }
    }

    pub fn restaurant_name(&self) -> String {
        let state = self.state.lock().unwrap();
        state.restaurant_name.clone()
    }

    pub fn toggle_dark_mode(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.dark_mode = !state.settings.dark_mode;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_hide_allergens(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.hide_allergens = !state.settings.hide_allergens;
        let _ = save_settings(&state.settings);
    }

    pub fn set_hover_point(&self, x: i32, y: i32) {
        let mut point = self.hover_point.lock().unwrap();
        *point = Some((x, y));
    }

    pub fn clear_hover_point(&self) {
        let mut point = self.hover_point.lock().unwrap();
        *point = None;
    }

    pub fn hover_point(&self) -> Option<(i32, i32)> {
        let point = self.hover_point.lock().unwrap();
        *point
    }

    pub fn set_context_menu_open(&self, open: bool) {
        let mut flag = self.context_menu_open.lock().unwrap();
        *flag = open;
    }

    pub fn is_context_menu_open(&self) -> bool {
        let flag = self.context_menu_open.lock().unwrap();
        *flag
    }
}

pub fn now_epoch_ms() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}
