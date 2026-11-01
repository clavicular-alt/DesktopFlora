use std::cell::RefCell;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
use windows::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Input::{RAWINPUTDEVICE, RIDEV_INPUTSINK, RegisterRawInputDevices};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use crate::log;
use crate::outside;
use crate::prefs::{self, COLORS, FRAME_CAPS, GLITTERS, MOUSE_WINDS, OPACITIES, SCHEDULES, SIZES, Settings, WINDS};
use crate::snow::{self, Command, Links, WM_NO_SNOWMAN};
use crate::text::{fill, t};
use crate::tracker::{COVER_ONLY, IGNORED, class_name, process_name};
use crate::update::{self, Release};
use crate::welcome::{self, Handlers};
use snowhush_core::simulation::{Look, Snowfall, Weather};
use snowhush_core::weather::Conditions;

const CLASS: PCWSTR = w!("SnowhushController");
const TITLE: PCWSTR = w!("DesktopFlora");
const WM_TRAY: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const ID_SNOWFALL: u32 = 100;
const ID_ACCUMULATE: u32 = 200;
const ID_MELT: u32 = 201;
const ID_SCREENSHOTS: u32 = 202;
const ID_LOGIN: u32 = 203;
const ID_WINDOWS: u32 = 204;
const ID_BEHIND: u32 = 205;
const ID_UPDATES: u32 = 206;
const ID_CLEAR: u32 = 207;
const ID_TYPING: u32 = 208;
const ID_SHORTCUTS: u32 = 209;
const ID_SNOWBALL: u32 = 210;
const ID_SNOWMAN: u32 = 211;
const ID_QUIT: u32 = 299;
const ID_WEATHER: u32 = 300;
const ID_SEASONAL: u32 = 309;
const ID_LOCAL_WEATHER: u32 = 310;
const ID_SIZE: u32 = 320;
const ID_WIND: u32 = 330;
const ID_OPACITY: u32 = 340;
const ID_FRAME_CAP: u32 = 350;
const ID_MOUSE_WIND: u32 = 360;
const ID_GLITTER: u32 = 370;
const ID_COLOR: u32 = 380;
const ID_SCHEDULE: u32 = 390;
const ID_EXCLUDE: u32 = 400;
/// Each excluded program, from here up: choosing one lets the snow back on it.
const ID_EXCLUDED: u32 = 401;
const ID_LAST: u32 = 499;
const TIMER_WELCOME: usize = 1;
const TIMER_UPDATES: usize = 2;
const TIMER_WEATHER: usize = 3;
const TIMER_CLICK: usize = 4;
const TIMER_MINUTE: usize = 5;
const WM_UPDATE: u32 = WM_APP + 2;
const WM_WEATHER: u32 = WM_APP + 3;
const WM_INSTALLED: u32 = WM_APP + 4;
const WEATHER_INTERVAL: u32 = 30 * 60 * 1000;
/// The shell's message for a click on the tray notification.
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const DAY: u32 = 24 * 60 * 60 * 1000;
const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_CLEAR: i32 = 2;
const HOTKEY_WEATHER: i32 = 3;
const HOTKEY_SNOWBALL: i32 = 4;
const HOTKEY_SNOWMAN: i32 = 5;
/// Ctrl+Alt plus a letter, by hotkey id.
const HOTKEYS: [(i32, u32); 5] =
    [(HOTKEY_TOGGLE, b'S' as u32), (HOTKEY_CLEAR, b'C' as u32), (HOTKEY_WEATHER, b'W' as u32), (HOTKEY_SNOWBALL, b'B' as u32), (HOTKEY_SNOWMAN, b'M' as u32)];

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// When the mouse was last used, from raw input: input without it was the keyboard.
pub static MOUSE_INPUT_AT: AtomicU32 = AtomicU32::new(0);

struct App {
    settings: Settings,
    commands: Sender<Command>,
    wake: HANDLE,
    snow: Option<JoinHandle<()>>,
    icon: HICON,
    locked: bool,
    display_off: bool,
    away: bool,
    /// The update the tray last announced.
    update: Option<Release>,
    /// Programs whose windows get no snow, by lowercase executable name.
    excluded: Vec<String>,
    /// The weather outside, while Snowfall follows it.
    outside: Option<Conditions>,
    /// What the snow thread was last told, so only changes are sent.
    sent: Option<(Snowfall, Look, bool)>,
    /// The snowfall to come back to when the snow is switched on again.
    snowfall_before_off: Snowfall,
    /// Where the tray icon was clicked, for the menu once it's clear the click wasn't a double-click.
    click: (i32, i32),
    double_clicked_at: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// The program last in front other than Snowhush and the shell, which the menu offers to keep the snow off.
    static LAST_APP: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The tray icon and its menu, and the system events that pause the snow, on the main thread;
/// the snow itself runs on its own thread so menus and other modal loops never stall it.
pub fn run() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        let window = CreateWindowExW(WINDOW_EX_STYLE(0), CLASS, TITLE, WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(instance.into()), None)?;
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let settings = Settings::load();
        let (commands, receiver) = mpsc::channel();
        let wake = CreateEventW(None, false, false, None)?;
        let links = Links { wake: wake.0 as isize, controller: window.0 as isize };
        let snow = thread::Builder::new().name("snow".into()).spawn(move || snow::run(receiver, links, settings)).ok();
        let icon = tray_icon();
        add_tray_icon(window, icon);
        let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
        let _ = RegisterPowerSettingNotification(HANDLE(window.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
        if !settings.welcomed {
            SetTimer(Some(window), TIMER_WELCOME, 1200, None);
        }
        SetTimer(Some(window), TIMER_UPDATES, 30_000, None);
        SetTimer(Some(window), TIMER_MINUTE, 60_000, None);
        let _ = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        // Mouse input only, never the keyboard: it tells Fade While Typing that input came from the mouse.
        let mouse = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: window };
        let _ = RegisterRawInputDevices(&[mouse], size_of::<RAWINPUTDEVICE>() as u32);
        let excluded = prefs::excluded_apps();
        let app = App {
            settings,
            commands,
            wake,
            snow,
            icon,
            locked: false,
            display_off: false,
            away: false,
            update: None,
            excluded: excluded.clone(),
            outside: None,
            sent: None,
            snowfall_before_off: if settings.snowfall == Snowfall::Off { Snowfall::Steady } else { settings.snowfall },
            click: (0, 0),
            double_clicked_at: 0,
        };
        send(&app, Command::Excluded(excluded));
        APP.set(Some(app));
        apply();
        register_hotkeys(window, settings.shortcuts);
        prefs::open_at_login_by_default();
        if settings.local_weather {
            check_weather(window, false);
            SetTimer(Some(window), TIMER_WEATHER, WEATHER_INTERVAL, None);
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TRAY => {
            let event = (lparam.0 & 0xffff) as u32;
            let x = (wparam.0 & 0xffff) as i16 as i32;
            let y = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            let now = unsafe { GetTickCount() };
            let double_click = unsafe { GetDoubleClickTime() };
            if event == WM_CONTEXTMENU || event == NIN_KEYSELECT {
                show_menu(window, x, y);
            } else if event == NIN_SELECT {
                // A click may be the first of a double-click: the menu waits until it's clear it isn't.
                let after_double = APP.with_borrow(|app| app.as_ref().is_some_and(|app| now.wrapping_sub(app.double_clicked_at) < double_click));
                if !after_double {
                    APP.with_borrow_mut(|app| {
                        if let Some(app) = app {
                            app.click = (x, y);
                        }
                    });
                    unsafe { SetTimer(Some(window), TIMER_CLICK, double_click, None) };
                }
            } else if event == WM_LBUTTONDBLCLK {
                unsafe {
                    let _ = KillTimer(Some(window), TIMER_CLICK);
                }
                APP.with_borrow_mut(|app| {
                    if let Some(app) = app {
                        app.double_clicked_at = now;
                    }
                });
                toggle_snow();
            } else if event == NIN_BALLOONUSERCLICK
                && let Some(release) = APP.with_borrow(|app| app.as_ref().and_then(|app| app.update.clone()))
            {
                offer(window, release);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_CLICK => {
            unsafe {
                let _ = KillTimer(Some(window), TIMER_CLICK);
            }
            if let Some((x, y)) = APP.with_borrow(|app| app.as_ref().map(|app| app.click)) {
                show_menu(window, x, y);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_MINUTE => {
            // The schedule and the seasons follow the clock.
            apply();
            LRESULT(0)
        }
        WM_HOTKEY => {
            hotkey(wparam.0 as i32);
            LRESULT(0)
        }
        WM_INPUT => {
            MOUSE_INPUT_AT.store(unsafe { GetTickCount() }, Ordering::Relaxed);
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_NO_SNOWMAN => {
            notify(window, &t("Not enough snow here"), &t("Let more snow settle, or try where the drift is deepest."));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_WEATHER => {
            check_weather(window, false);
            LRESULT(0)
        }
        WM_WEATHER => {
            let result = unsafe { Box::from_raw(lparam.0 as *mut std::result::Result<Conditions, String>) };
            weather_arrived(window, *result, wparam.0 != 0);
            LRESULT(0)
        }
        WM_INSTALLED => {
            let result = unsafe { Box::from_raw(lparam.0 as *mut (Release, std::result::Result<(), String>)) };
            let (release, result) = *result;
            // On success the installer closes this copy and opens the new one.
            if let Err(error) = result {
                install_failed(window, &release, &error);
            }
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            update(|app| match wparam.0 as u32 {
                WTS_SESSION_LOCK => app.locked = true,
                WTS_SESSION_UNLOCK => app.locked = false,
                WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => app.away = true,
                WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT => app.away = false,
                _ => {}
            });
            LRESULT(0)
        }
        WM_POWERBROADCAST if wparam.0 as u32 == PBT_POWERSETTINGCHANGE => {
            let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
            if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE {
                let off = setting.Data[0] == 0;
                update(|app| app.display_off = off);
            }
            LRESULT(1)
        }
        WM_CLOSE => {
            quit(window);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_UPDATES => {
            unsafe { SetTimer(Some(window), TIMER_UPDATES, DAY, None) };
            check_for_updates(window, false);
            LRESULT(0)
        }
        WM_UPDATE => {
            let result = unsafe { Box::from_raw(lparam.0 as *mut std::result::Result<Option<Release>, String>) };
            updated(window, *result, wparam.0 != 0);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_WELCOME => {
            unsafe {
                let _ = KillTimer(Some(window), TIMER_WELCOME);
            }
            welcome::show(Handlers {
                scatter: Box::new(|pieces| {
                    APP.with_borrow(|app| {
                        if let Some(app) = app {
                            send(app, Command::Scatter(pieces));
                        }
                    })
                }),
                finished: Box::new(|| {
                    APP.with_borrow_mut(|app| {
                        let Some(app) = app else { return };
                        app.settings.welcomed = true;
                        app.settings.save();
                    })
                }),
                skipped: Box::new(|| {
                    APP.with_borrow(|app| {
                        if let Some(app) = app {
                            send(app, Command::ReleaseSnowfall);
                        }
                    })
                }),
            });
            LRESULT(0)
        }
        _ if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            APP.with_borrow(|app| {
                if let Some(app) = app {
                    add_tray_icon(window, app.icon);
                    send(app, Command::ShellRestarted);
                }
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Changes what pauses the snow and tells the snow thread whether it may run.
fn update(change: impl FnOnce(&mut App)) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        change(app);
        let active = !app.locked && !app.display_off && !app.away;
        send(app, Command::Active(active));
    });
}

fn send(app: &App, command: Command) {
    let _ = app.commands.send(command);
    unsafe {
        let _ = SetEvent(app.wake);
    }
}

/// Works out what should fall now, from the menu, the weather outside, the season and the schedule,
/// and tells the snow thread whatever changed.
fn apply() {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        let now = unsafe { GetLocalTime() };
        let snowfall = match app.outside {
            Some(outside) if app.settings.local_weather => outside.snowfall,
            _ => app.settings.snowfall,
        };
        let look = effective_look(&app.settings, app.outside, now.wMonth as u32);
        let scheduled = app.settings.scheduled(now.wHour as u32);
        let sent = app.sent;
        if sent.map(|(s, _, _)| s) != Some(snowfall) {
            send(app, Command::Snowfall(snowfall));
        }
        if sent.map(|(_, l, _)| l) != Some(look) {
            send(app, Command::Look(look));
        }
        if sent.map(|(_, _, on)| on) != Some(scheduled) {
            send(app, Command::Scheduled(scheduled));
        }
        app.sent = Some((snowfall, look, scheduled));
    });
}

/// The look chosen in the menu, with the weather and wind from outside while following it, or the season's weather.
fn effective_look(settings: &Settings, outside: Option<Conditions>, month: u32) -> Look {
    let look = settings.look();
    match outside {
        Some(outside) if settings.local_weather => Look { weather: outside.weather, wind: look.wind * outside.wind, ..look },
        _ if settings.seasonal => Look { weather: season(month), ..look },
        _ => look,
    }
}

fn season(month: u32) -> Weather {
    match month {
        3..=5 => Weather::Blossoms,
        6..=8 => Weather::Rain,
        9..=11 => Weather::Leaves,
        _ => Weather::Snow,
    }
}

/// The weather falling now, as drawn.
fn current_weather(app: &App) -> Weather {
    app.sent.map_or(app.settings.weather, |(_, look, _)| look.weather)
}

/// Snow off, or back on at the snowfall it had: for the double-click and Ctrl+Alt+S.
fn toggle_snow() {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        let falling = app.sent.is_some_and(|(snowfall, _, _)| snowfall != Snowfall::Off);
        if falling {
            if app.settings.snowfall != Snowfall::Off {
                app.snowfall_before_off = app.settings.snowfall;
            }
            app.settings.snowfall = Snowfall::Off;
            stop_following(app);
        } else {
            app.settings.snowfall = app.snowfall_before_off;
        }
        app.settings.save();
    });
    apply();
}

fn hotkey(id: i32) {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    match id {
        HOTKEY_TOGGLE => toggle_snow(),
        HOTKEY_CLEAR => with_app(|app| send(app, Command::ClearSnow(!prefs::animations_enabled()))),
        HOTKEY_WEATHER => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                let current = Weather::ALL.iter().position(|&w| w == current_weather(app)).unwrap_or(0);
                app.settings.weather = Weather::ALL[(current + 1) % Weather::ALL.len()];
                app.settings.seasonal = false;
                stop_following(app);
                app.settings.save();
            });
            apply();
        }
        HOTKEY_SNOWBALL => with_app(|app| send(app, Command::Snowball(cursor.x, cursor.y))),
        HOTKEY_SNOWMAN => with_app(|app| send(app, Command::Snowman(cursor.x, cursor.y))),
        _ => {}
    }
}

fn register_hotkeys(window: HWND, on: bool) {
    for (id, key) in HOTKEYS {
        unsafe {
            let _ = UnregisterHotKey(Some(window), id);
            if on && RegisterHotKey(Some(window), id, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0), key).is_err() {
                log!("shortcut Ctrl+Alt+{} is taken by another program", key as u8 as char);
            }
        }
    }
}

fn with_app(action: impl FnOnce(&App)) {
    APP.with_borrow(|app| {
        if let Some(app) = app {
            action(app);
        }
    });
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// A menu being built, with its texts in the person's language.
struct Menu(HMENU);

impl Menu {
    fn new() -> Option<Menu> {
        unsafe { CreatePopupMenu() }.ok().map(Menu)
    }

    fn item(&self, id: u32, text: &str, flags: MENU_ITEM_FLAGS) {
        let text = wide(&t(text));
        unsafe {
            let _ = AppendMenuW(self.0, MF_STRING | flags, id as usize, PCWSTR(text.as_ptr()));
        }
    }

    fn check(&self, id: u32, text: &str, on: bool, enabled: bool) {
        self.item(id, text, if on { MF_CHECKED } else { MF_UNCHECKED } | if enabled { MF_ENABLED } else { MF_GRAYED });
    }

    fn note(&self, text: &str) {
        self.item(0, text, MF_GRAYED);
    }

    fn separator(&self) {
        unsafe {
            let _ = AppendMenuW(self.0, MF_SEPARATOR, 0, PCWSTR::null());
        }
    }

    fn submenu(&self, text: &str, submenu: Menu) {
        let text = wide(&t(text));
        unsafe {
            let _ = AppendMenuW(self.0, MF_POPUP, submenu.0.0 as usize, PCWSTR(text.as_ptr()));
        }
    }

    /// Options of which one is chosen, marked with a dot; none is marked when `chosen` is `None`.
    fn radio(options: &[&str], first: u32, chosen: Option<usize>) -> Option<Menu> {
        let menu = Menu::new()?;
        for (i, option) in options.iter().enumerate() {
            menu.item(first + i as u32, option, MF_ENABLED);
        }
        if let Some(chosen) = chosen {
            unsafe {
                let _ = CheckMenuRadioItem(menu.0, first, first + options.len() as u32 - 1, first + chosen as u32, MF_BYCOMMAND.0);
            }
        }
        Some(menu)
    }
}

fn show_menu(window: HWND, x: i32, y: i32) {
    let Some((settings, excluded, weather)) =
        APP.with_borrow(|app| app.as_ref().map(|app| (app.settings, app.excluded.clone(), current_weather(app))))
    else {
        return;
    };
    let login = prefs::opens_at_login();
    let last = LAST_APP.with_borrow(Clone::clone).filter(|app| !excluded.contains(app));
    let Some(chosen) = build_menu(&settings, &excluded, weather, login, last.as_deref()).map(|menu| unsafe {
        // A tray menu only closes on an outside click while its owner is in the foreground.
        let _ = SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(menu.0, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY).0, x, y, window, None).0 as u32;
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu.0);
        chosen
    }) else {
        return;
    };
    choose(window, chosen, login, last);
}

fn build_menu(settings: &Settings, excluded: &[String], weather: Weather, login: bool, last: Option<&str>) -> Option<Menu> {
    let menu = Menu::new()?;
    let local = settings.local_weather;
    let names = |options: &[(&'static str, f32)]| options.iter().map(|(name, _)| *name).collect::<Vec<_>>();

    let snowfall_titles: Vec<&str> = Snowfall::ALL.iter().map(|s| s.title()).collect();
    // Following the weather outside, the chosen snowfall waits; none is marked.
    let snowfall = (!local).then(|| Snowfall::ALL.iter().position(|&s| s == settings.snowfall).unwrap_or(0));
    menu.submenu("Snowfall", Menu::radio(&snowfall_titles, ID_SNOWFALL, snowfall)?);

    let weather_titles: Vec<&str> = Weather::ALL.iter().map(|w| w.title()).collect();
    let chosen_weather = (!local && !settings.seasonal).then(|| Weather::ALL.iter().position(|&w| w == settings.weather).unwrap_or(0));
    let weather_menu = Menu::radio(&weather_titles, ID_WEATHER, chosen_weather)?;
    weather_menu.separator();
    weather_menu.check(ID_SEASONAL, "Seasonal", settings.seasonal, true);
    weather_menu.check(ID_LOCAL_WEATHER, "Match Local Weather", local, true);
    menu.submenu("Weather", weather_menu);

    let look = Menu::new()?;
    look.submenu("Flake Size", Menu::radio(&names(&SIZES), ID_SIZE, Some(settings.size))?);
    look.submenu("Wind", Menu::radio(&names(&WINDS), ID_WIND, Some(settings.wind))?);
    look.submenu("Mouse Wind", Menu::radio(&names(&MOUSE_WINDS), ID_MOUSE_WIND, Some(settings.mouse_wind))?);
    let colors: Vec<&str> = COLORS.iter().map(|(name, _)| *name).collect();
    look.submenu("Color", Menu::radio(&colors, ID_COLOR, Some(settings.color))?);
    look.submenu("Glitter", Menu::radio(&names(&GLITTERS), ID_GLITTER, Some(settings.glitter))?);
    look.submenu("Opacity", Menu::radio(&names(&OPACITIES), ID_OPACITY, Some(settings.opacity))?);
    look.separator();
    let caps: Vec<&str> = FRAME_CAPS.iter().map(|(name, _)| *name).collect();
    look.submenu("Frame Rate", Menu::radio(&caps, ID_FRAME_CAP, Some(settings.frame_cap))?);
    look.check(ID_TYPING, "Fade While Typing", settings.fade_while_typing, true);
    menu.submenu("Look", look);

    let fun = Menu::new()?;
    fun.item(ID_SNOWBALL, "Throw Snowball", MF_ENABLED);
    fun.item(ID_SNOWMAN, "Build Snowman", MF_ENABLED);
    menu.submenu("Fun", fun);
    menu.separator();

    // Rain runs off: there is nothing for it to pile up or melt.
    let settles = weather != Weather::Rain;
    menu.check(ID_ACCUMULATE, "Accumulate", settings.accumulate, settles);
    menu.check(ID_MELT, "Melt Snow", settings.melt, settles && settings.accumulate);
    menu.check(ID_WINDOWS, "Snow on Windows", settings.snow_on_windows, !settings.behind_windows);
    menu.item(ID_CLEAR, "Clear Snow", MF_ENABLED);
    menu.separator();

    menu.check(ID_BEHIND, "Behind Windows", settings.behind_windows, true);
    menu.check(ID_SCREENSHOTS, "Show in Screenshots", settings.show_in_screenshots, true);
    let apps = Menu::new()?;
    match last {
        Some(app) => apps.item(ID_EXCLUDE, &fill(&t("Keep Snow Off {}"), &[app]), MF_ENABLED),
        None => apps.note("Switch to a program, then come back here"),
    }
    if !excluded.is_empty() {
        apps.separator();
        for (i, app) in excluded.iter().enumerate() {
            apps.item(ID_EXCLUDED + i as u32, app, MF_CHECKED);
        }
    }
    menu.submenu("Snow-Free Programs", apps);
    let schedules: Vec<&str> = SCHEDULES.iter().map(|(name, _, _)| *name).collect();
    menu.submenu("Schedule", Menu::radio(&schedules, ID_SCHEDULE, Some(settings.schedule))?);
    let shortcuts = Menu::new()?;
    shortcuts.check(ID_SHORTCUTS, "Enable Shortcuts", settings.shortcuts, true);
    shortcuts.separator();
    for line in [
        "Ctrl+Alt+S  Snow on/off",
        "Ctrl+Alt+C  Clear snow",
        "Ctrl+Alt+W  Next weather",
        "Ctrl+Alt+B  Snowball at the pointer",
        "Ctrl+Alt+M  Snowman at the pointer",
        "Double-click the tray icon: snow on/off",
    ] {
        shortcuts.note(line);
    }
    menu.submenu("Keyboard Shortcuts", shortcuts);
    menu.check(ID_LOGIN, "Open at Login", login, true);
    menu.separator();
    menu.item(ID_UPDATES, "Check for Updates…", MF_ENABLED);
    menu.item(ID_QUIT, "Quit", MF_ENABLED);
    Some(menu)
}

/// Remembers the program that came to the front, unless it is Snowhush or part of the shell.
unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, window: HWND, _: i32, _: i32, _: u32, _: u32) {
    let class = class_name(window);
    if IGNORED.contains(&class.as_str()) || COVER_ONLY.contains(&class.as_str()) {
        return;
    }
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    if process == 0 || process == std::process::id() {
        return;
    }
    let name = process_name(process).to_lowercase();
    // Without the right to read its name, a process shows as its number, which can't be kept off.
    if name.ends_with(".exe") && name != "explorer.exe" {
        LAST_APP.set(Some(name));
    }
}

fn set_excluded(change: impl FnOnce(&mut Vec<String>)) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        change(&mut app.excluded);
        prefs::set_excluded_apps(&app.excluded);
        send(app, Command::Excluded(app.excluded.clone()));
    });
}

/// Index into a list of options from a menu id, if `chosen` falls in its range.
fn option(chosen: u32, first: u32, count: usize) -> Option<usize> {
    let i = chosen.checked_sub(first)? as usize;
    (i < count).then_some(i)
}

fn choose(window: HWND, chosen: u32, login: bool, last: Option<String>) {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    match chosen {
        0 => {}
        ID_QUIT => quit(window),
        ID_UPDATES => check_for_updates(window, true),
        ID_CLEAR => with_app(|app| send(app, Command::ClearSnow(!prefs::animations_enabled()))),
        // From the menu, the snowball flies up from where the menu was, and the snowman stands below it.
        ID_SNOWBALL => with_app(|app| send(app, Command::Snowball(cursor.x - 250, cursor.y - 350))),
        ID_SNOWMAN => with_app(|app| send(app, Command::Snowman(cursor.x - 80, cursor.y))),
        ID_LOGIN => {
            let status = prefs::set_opens_at_login(!login);
            log!("open at login {}: {status:?}", !login);
        }
        ID_EXCLUDE => {
            if let Some(app) = last {
                set_excluded(|apps| apps.push(app));
            }
        }
        ID_EXCLUDED..=ID_LAST => set_excluded(|apps| {
            let i = (chosen - ID_EXCLUDED) as usize;
            if i < apps.len() {
                apps.remove(i);
            }
        }),
        ID_LOCAL_WEATHER => {
            let on = !APP.with_borrow(|app| app.as_ref().is_some_and(|app| app.settings.local_weather));
            set_local_weather(window, on);
            if on {
                check_weather(window, true);
            }
        }
        ID_SHORTCUTS => {
            let on = APP.with_borrow_mut(|app| {
                let app = app.as_mut()?;
                app.settings.shortcuts = !app.settings.shortcuts;
                app.settings.save();
                Some(app.settings.shortcuts)
            });
            if let Some(on) = on {
                register_hotkeys(window, on);
            }
        }
        _ => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                let settings = &mut app.settings;
                let command = if let Some(i) = option(chosen, ID_SNOWFALL, Snowfall::ALL.len()) {
                    settings.snowfall = Snowfall::ALL[i];
                    settings.local_weather = false;
                    app.outside = None;
                    None
                } else if let Some(i) = option(chosen, ID_WEATHER, Weather::ALL.len()) {
                    // Choosing by hand stops following the weather outside and the seasons.
                    settings.weather = Weather::ALL[i];
                    settings.seasonal = false;
                    settings.local_weather = false;
                    app.outside = None;
                    None
                } else if chosen == ID_SEASONAL {
                    settings.seasonal = !settings.seasonal;
                    None
                } else if let Some(i) = option(chosen, ID_SIZE, SIZES.len()) {
                    settings.size = i;
                    None
                } else if let Some(i) = option(chosen, ID_WIND, WINDS.len()) {
                    settings.wind = i;
                    None
                } else if let Some(i) = option(chosen, ID_MOUSE_WIND, MOUSE_WINDS.len()) {
                    settings.mouse_wind = i;
                    None
                } else if let Some(i) = option(chosen, ID_COLOR, COLORS.len()) {
                    settings.color = i;
                    None
                } else if let Some(i) = option(chosen, ID_SCHEDULE, SCHEDULES.len()) {
                    settings.schedule = i;
                    None
                } else if let Some(i) = option(chosen, ID_GLITTER, GLITTERS.len()) {
                    settings.glitter = i;
                    Some(Command::Glitter(settings.glitter()))
                } else if let Some(i) = option(chosen, ID_OPACITY, OPACITIES.len()) {
                    settings.opacity = i;
                    Some(Command::Opacity(settings.opacity()))
                } else if let Some(i) = option(chosen, ID_FRAME_CAP, FRAME_CAPS.len()) {
                    settings.frame_cap = i;
                    Some(Command::FrameCap(settings.frame_cap()))
                } else {
                    match chosen {
                        ID_ACCUMULATE => {
                            settings.accumulate = !settings.accumulate;
                            Some(Command::Accumulate(settings.accumulate))
                        }
                        ID_MELT => {
                            settings.melt = !settings.melt;
                            Some(Command::Melt(settings.melt))
                        }
                        ID_WINDOWS => {
                            settings.snow_on_windows = !settings.snow_on_windows;
                            Some(Command::SnowOnWindows(settings.snow_on_windows))
                        }
                        ID_BEHIND => {
                            settings.behind_windows = !settings.behind_windows;
                            Some(Command::BehindWindows(settings.behind_windows))
                        }
                        ID_SCREENSHOTS => {
                            settings.show_in_screenshots = !settings.show_in_screenshots;
                            Some(Command::ShowInScreenshots(settings.show_in_screenshots))
                        }
                        ID_TYPING => {
                            settings.fade_while_typing = !settings.fade_while_typing;
                            Some(Command::FadeWhileTyping(settings.fade_while_typing))
                        }
                        _ => return,
                    }
                };
                settings.save();
                if settings.snowfall != Snowfall::Off {
                    app.snowfall_before_off = settings.snowfall;
                }
                if let Some(command) = command {
                    send(app, command);
                }
            });
            // Snowfall, weather, look and schedule go through here, which sends only what changed.
            apply();
            if !APP.with_borrow(|app| app.as_ref().is_some_and(|app| app.settings.local_weather)) {
                unsafe {
                    let _ = KillTimer(Some(window), TIMER_WEATHER);
                }
            }
        }
    }
}

fn stop_following(app: &mut App) {
    app.settings.local_weather = false;
    app.outside = None;
}

fn set_local_weather(window: HWND, on: bool) {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        app.settings.local_weather = on;
        app.settings.save();
        if !on {
            app.outside = None;
        }
    });
    apply();
    unsafe {
        if on {
            SetTimer(Some(window), TIMER_WEATHER, WEATHER_INTERVAL, None);
        } else {
            let _ = KillTimer(Some(window), TIMER_WEATHER);
        }
    }
}

/// Asks for the weather outside on another thread and reports back to the window with `WM_WEATHER`.
fn check_weather(window: HWND, asked: bool) {
    let target = window.0 as isize;
    thread::spawn(move || {
        // WinRT needs the thread in an apartment before the location can be asked for.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let result = Box::new(outside::now());
        unsafe {
            let _ = PostMessageW(Some(HWND(target as _)), WM_WEATHER, WPARAM(asked as usize), LPARAM(Box::into_raw(result) as isize));
        }
    });
}

fn weather_arrived(window: HWND, result: std::result::Result<Conditions, String>, asked: bool) {
    match result {
        Ok(conditions) => {
            APP.with_borrow_mut(|app| {
                let Some(app) = app else { return };
                if app.settings.local_weather {
                    log!("outside: {conditions:?}");
                    app.outside = Some(conditions);
                }
            });
            apply();
        }
        Err(error) => {
            log!("weather failed: {error}");
            // A check that fails later keeps the last weather; one that fails as it's turned on turns it off again.
            if asked {
                set_local_weather(window, false);
                message(window, &fill(&t("Couldn't get the local weather.\n\n{}"), &[&error]), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

/// Checks on another thread and reports back to the window with `WM_UPDATE`.
fn check_for_updates(window: HWND, asked: bool) {
    let target = window.0 as isize;
    thread::spawn(move || {
        let result = Box::new(update::check());
        unsafe {
            let _ = PostMessageW(Some(HWND(target as _)), WM_UPDATE, WPARAM(asked as usize), LPARAM(Box::into_raw(result) as isize));
        }
    });
}

/// A check someone asked for always answers; the daily one only speaks up, once, about a new version.
fn updated(window: HWND, result: std::result::Result<Option<Release>, String>, asked: bool) {
    let current = env!("CARGO_PKG_VERSION");
    match result {
        Ok(Some(release)) if asked => offer(window, release),
        Ok(Some(release)) => {
            let fresh = APP.with_borrow_mut(|app| {
                let app = app.as_mut()?;
                (app.update.as_ref().map(|known| &known.url) != Some(&release.url)).then(|| app.update = Some(release.clone()))
            });
            if fresh.is_some() {
                notify(window, &fill(&t("DesktopFlora {} is available"), &[&release.version]), &t("Click to install it."));
            }
        }
        Ok(None) if asked => {
            message(window, &fill(&t("You're up to date.\n\nDesktopFlora {} is the latest version."), &[current]), MB_OK | MB_ICONINFORMATION);
        }
        Ok(None) => {}
        Err(error) => {
            log!("update check failed: {error}");
            if asked {
                message(window, &fill(&t("Couldn't check for updates.\n\n{}"), &[&error]), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

/// Offers a new version: installed in place when its release publishes a checksum to check the download against,
/// otherwise from its page in the browser.
fn offer(window: HWND, release: Release) {
    let current = env!("CARGO_PKG_VERSION");
    if release.digest.is_none() {
        let text = fill(&t("DesktopFlora {} is available. You have {}.\n\nOpen the download page?"), &[&release.version, current]);
        if message(window, &text, MB_YESNO | MB_ICONINFORMATION) == IDYES {
            update::open(&release.page);
        }
        return;
    }
    let text = fill(
        &t("DesktopFlora {} is available. You have {}.\n\nInstall it now? DesktopFlora downloads it, checks it against the release's checksum and restarts."),
        &[&release.version, current],
    );
    if message(window, &text, MB_YESNO | MB_ICONINFORMATION) != IDYES {
        return;
    }
    let target = window.0 as isize;
    thread::spawn(move || {
        let result = update::install(&release);
        let report = Box::new((release, result));
        unsafe {
            let _ = PostMessageW(Some(HWND(target as _)), WM_INSTALLED, WPARAM(0), LPARAM(Box::into_raw(report) as isize));
        }
    });
}

fn install_failed(window: HWND, release: &Release, error: &str) {
    log!("update failed: {error}");
    let text = fill(&t("Couldn't install the update.\n\n{}\n\nOpen the download page instead?"), &[error]);
    if message(window, &text, MB_YESNO | MB_ICONWARNING) == IDYES {
        update::open(&release.page);
    }
}

fn message(window: HWND, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let text = wide(text);
    unsafe { MessageBoxW(Some(window), PCWSTR(text.as_ptr()), TITLE, style | MB_SETFOREGROUND | MB_TOPMOST) }
}

/// A notification from the tray icon.
fn notify(window: HWND, title: &str, text: &str) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_INFO,
        ..Default::default()
    };
    for (slot, unit) in data.szInfoTitle.iter_mut().zip(title.encode_utf16().take(63)) {
        *slot = unit;
    }
    for (slot, unit) in data.szInfo.iter_mut().zip(text.encode_utf16().take(255)) {
        *slot = unit;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

fn quit(window: HWND) {
    remove_tray_icon(window);
    register_hotkeys(window, false);
    if let Some(mut app) = APP.with_borrow_mut(Option::take) {
        send(&app, Command::Quit);
        if let Some(snow) = app.snow.take() {
            let _ = snow.join();
        }
        unsafe {
            let _ = DestroyIcon(app.icon);
        }
    }
    unsafe { PostQuitMessage(0) };
}

/// The app icon from the executable's resources, at the tray's size for the display's scale.
fn tray_icon() -> HICON {
    unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem());
        let Ok(instance) = GetModuleHandleW(None) else { return HICON::default() };
        LoadImageW(Some(instance.into()), PCWSTR(1 as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map_or(HICON::default(), |icon| HICON(icon.0))
    }
}

fn add_tray_icon(window: HWND, icon: HICON) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip("DesktopFlora".encode_utf16()) {
        *slot = unit;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        let _ = Shell_NotifyIconW(NIM_ADD, &data);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
}

fn remove_tray_icon(window: HWND) {
    let data = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: window, uID: 1, ..Default::default() };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}
