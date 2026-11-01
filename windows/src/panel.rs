//! The settings panel that opens from the tray icon: drawn with Direct2D into a layered window, like the welcome card,
//! in the system's light or dark theme. Everything is laid out again on each paint, which also records where each
//! control is, so a click finds its action.

use std::cell::RefCell;
use std::mem::size_of;

use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory,
    ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, GetKeyState, MAPVK_VK_TO_VSC, MapVirtualKeyW, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_BACK, VK_CONTROL, VK_DELETE,
    VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};
use windows_numerics::Vector2;

use snowhush_core::simulation::{Snowfall, Weather};

use snowhush::card;
use crate::log;
use crate::prefs::{COLORS, FRAME_CAPS, GLITTERS, Hotkey, MOUSE_WINDS, OPACITIES, SCHEDULES, SHORTCUTS, SIZES, Settings, WINDS};
use crate::text::{fill, t};

const CLASS: PCWSTR = w!("DesktopFloraPanel");
const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 560.0;
/// Room around the panel for its shadow.
const MARGIN: f32 = 24.0;
const PAD: f32 = 20.0;
const TABS: [&str; 4] = ["Weather", "Look", "Behavior", "More"];
const WM_MOUSELEAVE: u32 = 0x02A3;
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

/// What a click on the panel asks for.
#[derive(Clone, Debug)]
pub enum Action {
    Toggle,
    Snowfall(Snowfall),
    Weather(Weather),
    Seasonal,
    LocalWeather,
    Size(usize),
    Wind(usize),
    MouseWind(usize),
    Color(usize),
    Glitter(usize),
    Opacity(usize),
    FrameCap(usize),
    Schedule(usize),
    Accumulate,
    Melt,
    SnowOnWindows,
    BehindWindows,
    Screenshots,
    FadeWhileTyping,
    OpenAtLogin,
    Clear,
    /// Keeps the snow off the program last in front.
    Exclude,
    /// Lets the snow back on the program at this place in the list.
    Include(usize),
    SetHotkey(usize, Option<Hotkey>),
    /// While a shortcut is being typed, the current ones let go of their keys, so typing one doesn't set it off.
    SuspendShortcuts(bool),
    Updates,
    Quit,
}

/// What the panel shows, read from the app each time it is drawn.
pub struct View {
    pub on: bool,
    pub settings: Settings,
    /// What falls now and how much, whichever way it was chosen.
    pub weather: Weather,
    pub amount: Snowfall,
    /// A line on where the weather comes from, as the season or the sky outside.
    pub status: String,
    pub excluded: Vec<String>,
    pub last_app: Option<String>,
    pub login: bool,
    /// Shortcuts another program already holds.
    pub taken: [bool; 3],
}

pub struct Handlers {
    pub view: Box<dyn Fn() -> Option<View>>,
    pub act: Box<dyn Fn(Action)>,
}

/// What a panel internally does with a click, beyond the app's actions.
#[derive(Clone, Debug)]
enum Hit {
    App(Action),
    Tab(usize),
    Capture(usize),
}

struct Panel {
    window: HWND,
    scale: f32,
    size: (i32, i32),
    tab: usize,
    hover: Option<usize>,
    hits: Vec<(D2D_RECT_F, Hit)>,
    /// The shortcut waiting for its new keys.
    capturing: Option<usize>,
    handlers: Handlers,
    d2d: ID2D1Factory,
    text: IDWriteFactory,
    imaging: IWICImagingFactory,
}

thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    /// The tab to open on next time, as the panel closes each time it loses focus.
    static LAST_TAB: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// When the panel last closed: a click on the tray icon closes it by taking the focus, and shouldn't open it again.
    static CLOSED_AT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Opens the panel by the tray icon, or closes it if it is open.
pub fn toggle(handlers: Handlers) {
    if PANEL.with_borrow(Option::is_some) {
        close();
        return;
    }
    if unsafe { GetTickCount() }.wrapping_sub(CLOSED_AT.get()) < 400 {
        return;
    }
    if let Err(error) = open(handlers) {
        log!("panel failed: {error}");
    }
}

/// Draws the panel again, if it is open, after something changed it from elsewhere, as a shortcut.
pub fn refresh() {
    paint();
}

pub fn close() {
    if let Some(panel) = PANEL.take() {
        LAST_TAB.set(panel.tab);
        CLOSED_AT.set(unsafe { GetTickCount() });
        if panel.capturing.is_some() {
            (panel.handlers.act)(Action::SuspendShortcuts(false));
        }
        unsafe {
            let _ = DestroyWindow(panel.window);
        }
    }
}

fn open(handlers: Handlers) -> Result<()> {
    card::register(CLASS, Some(procedure))?;
    let (scale, origin) = placement();
    let size = (((WIDTH + 2.0 * MARGIN) * scale).ceil() as i32, ((HEIGHT + 2.0 * MARGIN) * scale).ceil() as i32);
    unsafe {
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            w!("DesktopFlora"),
            WS_POPUP,
            origin.x,
            origin.y,
            size.0,
            size.1,
            None,
            None,
            None,
            None,
        )?;
        PANEL.set(Some(Panel {
            window,
            scale,
            size,
            tab: LAST_TAB.get(),
            hover: None,
            hits: Vec::new(),
            capturing: None,
            handlers,
            d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
            text: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
            imaging: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
        }));
        paint_at(Some(origin));
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    Ok(())
}

/// The display's scale, and where the panel goes: by the tray, on the side of the screen the taskbar is on.
fn placement() -> (f32, POINT) {
    unsafe {
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut dpi, mut _y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
        let scale = dpi.max(96) as f32 / 96.0;
        // The panel itself, without its shadow's margin, 12 DIPs in from the work area, by the pointer: next to the tray,
        // wherever the taskbar is.
        let (width, height, gap, margin) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32, (12.0 * scale) as i32, (MARGIN * scale) as i32);
        let RECT { left, top, right, bottom } = info.rcWork;
        let x = (cursor.x - width / 2).clamp(left + gap, (right - gap - width).max(left + gap));
        let y = if cursor.y >= bottom {
            bottom - gap - height
        } else if cursor.y < top {
            top + gap
        } else {
            (cursor.y - height / 2).clamp(top + gap, (bottom - gap - height).max(top + gap))
        };
        (scale, POINT { x: x - margin, y: y - margin })
    }
}

fn paint() {
    paint_at(None);
}

fn paint_at(position: Option<POINT>) {
    let painted = PANEL.with_borrow_mut(|panel| {
        let panel = panel.as_mut()?;
        let view = (panel.handlers.view)()?;
        match panel.render(&view) {
            Ok((pixels, hits)) => {
                panel.hits = hits;
                Some((panel.window, panel.size, pixels))
            }
            Err(error) => {
                log!("panel paint failed: {error}");
                None
            }
        }
    });
    if let Some((window, size, pixels)) = painted {
        card::present(window, size, &pixels, position);
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => {
            close();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let point = card::point(lparam);
            let changed = PANEL.with_borrow_mut(|panel| {
                let panel = panel.as_mut()?;
                let at = (point.x as f32 / panel.scale, point.y as f32 / panel.scale);
                let hover = panel.hits.iter().position(|(rect, _)| card::contains(rect, at));
                let changed = hover != panel.hover;
                panel.hover = hover;
                Some(changed)
            });
            if changed == Some(true) {
                let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: window, dwHoverTime: 0 };
                unsafe {
                    let _ = TrackMouseEvent(&mut track);
                }
                paint();
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            PANEL.with_borrow_mut(|panel| {
                if let Some(panel) = panel {
                    panel.hover = None;
                }
            });
            paint();
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let point = card::point(lparam);
            let hit = PANEL.with_borrow(|panel| {
                let panel = panel.as_ref()?;
                let at = (point.x as f32 / panel.scale, point.y as f32 / panel.scale);
                panel.hits.iter().find(|(rect, _)| card::contains(rect, at)).map(|(_, hit)| hit.clone())
            });
            if let Some(hit) = hit {
                click(hit);
            }
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            key(wparam.0 as u32);
            LRESULT(0)
        }
        // Alt alone would otherwise open the window menu while a shortcut is being typed.
        WM_SYSCHAR | WM_SYSKEYUP => LRESULT(0),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn click(hit: Hit) {
    match hit {
        Hit::Tab(tab) => {
            PANEL.with_borrow_mut(|panel| {
                if let Some(panel) = panel {
                    panel.tab = tab;
                    panel.capturing = None;
                }
            });
            paint();
        }
        Hit::Capture(index) => {
            let capturing = PANEL.with_borrow_mut(|panel| {
                let panel = panel.as_mut()?;
                panel.capturing = if panel.capturing == Some(index) { None } else { Some(index) };
                Some(panel.capturing.is_some())
            });
            if let Some(capturing) = capturing {
                suspend(capturing);
            }
            paint();
        }
        Hit::App(Action::Quit) => {
            let act = take_act();
            close();
            if let Some(act) = act {
                act(Action::Quit);
            }
        }
        Hit::App(action) => {
            // The handler may open a message box, which takes the focus and so closes the panel: act outside the borrow.
            let act = take_act();
            if let Some(act) = &act {
                act(action);
            }
            put_act(act);
            paint();
        }
    }
}

/// Takes the action handler out of the panel while it runs, so the panel can be borrowed, or closed, meanwhile.
fn take_act() -> Option<Box<dyn Fn(Action)>> {
    PANEL.with_borrow_mut(|panel| panel.as_mut().map(|panel| std::mem::replace(&mut panel.handlers.act, Box::new(|_| {}))))
}

fn put_act(act: Option<Box<dyn Fn(Action)>>) {
    let Some(act) = act else { return };
    PANEL.with_borrow_mut(|panel| {
        if let Some(panel) = panel {
            panel.handlers.act = act;
        }
    });
}

fn key(code: u32) {
    let capturing = PANEL.with_borrow(|panel| panel.as_ref().and_then(|panel| panel.capturing));
    let Some(index) = capturing else {
        if code == VK_ESCAPE.0 as u32 {
            close();
        }
        return;
    };
    let modifiers_only = [VK_CONTROL.0, VK_MENU.0, VK_SHIFT.0, VK_LWIN.0, VK_RWIN.0, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5].contains(&(code as u16));
    if modifiers_only {
        return;
    }
    let hotkey = if code == VK_ESCAPE.0 as u32 {
        stop_capturing();
        return;
    } else if code == VK_BACK.0 as u32 || code == VK_DELETE.0 as u32 {
        None
    } else {
        let down = |key: u16| unsafe { GetKeyState(key as i32) } < 0;
        let modifiers = if down(VK_CONTROL.0) { MOD_CONTROL } else { 0 }
            | if down(VK_MENU.0) { MOD_ALT } else { 0 }
            | if down(VK_SHIFT.0) { MOD_SHIFT } else { 0 }
            | if down(VK_LWIN.0) || down(VK_RWIN.0) { MOD_WIN } else { 0 };
        // A shortcut without Ctrl, Alt or Win would take the key away from typing.
        if modifiers & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 {
            return;
        }
        Some(Hotkey { modifiers, key: code })
    };
    stop_capturing();
    click(Hit::App(Action::SetHotkey(index, hotkey)));
}

fn stop_capturing() {
    let was = PANEL.with_borrow_mut(|panel| panel.as_mut().and_then(|panel| panel.capturing.take()));
    if was.is_some() {
        suspend(false);
    }
    paint();
}

fn suspend(on: bool) {
    let act = take_act();
    if let Some(act) = &act {
        act(Action::SuspendShortcuts(on));
    }
    put_act(act);
}

/// A shortcut as people read it, as "Ctrl+Alt+S", with the modifier names in the person's language.
pub fn describe(hotkey: Hotkey) -> String {
    let mut text = String::new();
    for (flag, name) in [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift"), (MOD_WIN, "Win")] {
        if hotkey.modifiers & flag != 0 {
            text.push_str(&t(name));
            text.push('+');
        }
    }
    let key = hotkey.key;
    let name = match key {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(key).map(String::from).unwrap_or_default(),
        0x70..=0x87 => format!("F{}", key - 0x6F),
        _ => unsafe {
            let scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC);
            let mut buffer = [0u16; 32];
            let length = GetKeyNameTextW((scan << 16) as i32, &mut buffer);
            if length > 0 { String::from_utf16_lossy(&buffer[..length as usize]) } else { format!("#{key}") }
        },
    };
    text + &name
}

/// The panel's colours, from the system's app theme.
struct Palette {
    background: [f32; 3],
    surface: [f32; 3],
    raised: [f32; 3],
    ink: [f32; 3],
    muted: [f32; 3],
    accent: [f32; 3],
    on_accent: [f32; 3],
    warning: [f32; 3],
    shadow: f32,
}

fn palette() -> Palette {
    let mut light = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_SUCCESS && light != 0 {
        Palette {
            background: [0.975, 0.978, 0.985],
            surface: [0.918, 0.925, 0.94],
            raised: [1.0, 1.0, 1.0],
            ink: [0.1, 0.11, 0.13],
            muted: [0.42, 0.44, 0.49],
            accent: [0.13, 0.46, 0.86],
            on_accent: [1.0, 1.0, 1.0],
            warning: [0.8, 0.22, 0.18],
            shadow: 0.018,
        }
    } else {
        Palette {
            background: [0.118, 0.125, 0.145],
            surface: [0.176, 0.184, 0.21],
            raised: [0.235, 0.245, 0.275],
            ink: [0.95, 0.955, 0.97],
            muted: [0.62, 0.64, 0.69],
            accent: [0.49, 0.77, 1.0],
            on_accent: [0.06, 0.1, 0.16],
            warning: [1.0, 0.48, 0.42],
            shadow: 0.03,
        }
    }
}

impl Panel {
    fn render(&self, view: &View) -> Result<(Vec<u8>, Vec<(D2D_RECT_F, Hit)>)> {
        let (width, height) = self.size;
        unsafe {
            let bitmap = self.imaging.CreateBitmap(width as u32, height as u32, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0 * self.scale,
                dpiY: 96.0 * self.scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties)?;
            target.BeginDraw();
            target.Clear(Some(&color([0.0; 3], 0.0)));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let mut painter = Painter { target: &target, text: &self.text, colors: palette(), hits: Vec::new(), hover: self.hover };
            let drawn = painter.panel(view, self.tab, self.capturing);
            let hits = std::mem::take(&mut painter.hits);
            drop(painter);
            target.EndDraw(None, None)?;
            drawn?;
            drop(target);
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            bitmap.CopyPixels(std::ptr::null(), (width * 4) as u32, &mut pixels)?;
            Ok((pixels, hits))
        }
    }
}

/// Draws the panel and records where its controls are.
struct Painter<'a> {
    target: &'a ID2D1RenderTarget,
    text: &'a IDWriteFactory,
    colors: Palette,
    hits: Vec<(D2D_RECT_F, Hit)>,
    hover: Option<usize>,
}

impl Painter<'_> {
    fn panel(&mut self, view: &View, tab: usize, capturing: Option<usize>) -> Result<()> {
        let frame = D2D_RECT_F { left: MARGIN, top: MARGIN, right: MARGIN + WIDTH, bottom: MARGIN + HEIGHT };
        // A soft shadow from stacked translucent outlines, lighter as they spread.
        for step in 0..12 {
            let spread = step as f32 * 1.5;
            let rect = D2D_RECT_F { left: frame.left - spread, top: frame.top - spread + 6.0, right: frame.right + spread, bottom: frame.bottom + spread + 6.0 };
            self.fill(rect, 14.0 + spread, [0.0; 3], self.colors.shadow)?;
        }
        self.fill(frame, 14.0, self.colors.background, 0.985)?;
        self.outline(frame, 14.0, [1.0; 3], if self.colors.background[0] > 0.5 { 0.0 } else { 0.07 })?;
        let (left, right) = (frame.left + PAD, frame.right - PAD);

        // Header: name, what falls now, and the switch.
        self.label("DesktopFlora", D2D_RECT_F { left, top: frame.top + 16.0, right, bottom: frame.top + 40.0 }, 18.0, true, self.colors.ink, LEADING)?;
        let falling = if view.on {
            fill(&t("{} · {}"), &[&t(view.weather.title()), &t(view.amount.title())])
        } else {
            t("Paused")
        };
        self.label(&falling, D2D_RECT_F { left, top: frame.top + 40.0, right: right - 60.0, bottom: frame.top + 58.0 }, 12.5, false, self.colors.muted, LEADING)?;
        let switch = D2D_RECT_F { left: right - 48.0, top: frame.top + 22.0, right, bottom: frame.top + 48.0 };
        self.switch(switch, view.on, true, Hit::App(Action::Toggle))?;

        // Tabs.
        let top = frame.top + 72.0;
        let tab_width = (right - left - 3.0 * 6.0) / 4.0;
        self.fill(D2D_RECT_F { left: left - 4.0, top: top - 4.0, right: right + 4.0, bottom: top + 34.0 }, 10.0, self.colors.surface, 1.0)?;
        for (i, name) in TABS.iter().enumerate() {
            let x = left + i as f32 * (tab_width + 6.0);
            let rect = D2D_RECT_F { left: x, top, right: x + tab_width, bottom: top + 30.0 };
            let chosen = i == tab;
            if chosen {
                self.fill(rect, 8.0, self.colors.raised, 1.0)?;
            }
            self.hit(rect, Hit::Tab(i), 8.0)?;
            let ink = if chosen { self.colors.ink } else { self.colors.muted };
            self.label(&t(name), rect, 12.5, chosen, ink, CENTER)?;
        }

        let mut y = top + 50.0;
        match tab {
            0 => self.weather_tab(view, left, right, &mut y)?,
            1 => self.look_tab(view, left, right, &mut y)?,
            2 => self.behavior_tab(view, left, right, &mut y)?,
            _ => self.more_tab(view, left, right, &mut y, capturing)?,
        }

        // Footer.
        let bottom = frame.bottom - 14.0;
        self.fill(D2D_RECT_F { left, top: bottom - 30.0, right, bottom: bottom - 29.0 }, 0.0, self.colors.muted, 0.2)?;
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        self.label(&version, D2D_RECT_F { left, top: bottom - 24.0, right: right - 100.0, bottom }, 12.0, false, self.colors.muted, LEADING)?;
        let quit = D2D_RECT_F { left: right - 90.0, top: bottom - 26.0, right, bottom };
        self.hit(quit, Hit::App(Action::Quit), 6.0)?;
        self.label(&t("Quit"), quit, 12.5, false, self.colors.muted, TRAILING)?;
        Ok(())
    }

    fn weather_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        self.heading("What falls", left, right, y)?;
        let fixed = !settings.local_weather && !settings.seasonal;
        let columns = 3.0;
        let width = (right - left - 2.0 * 8.0) / columns;
        let mut chips: Vec<(String, bool, Action)> =
            Weather::ALL.iter().map(|&weather| (t(weather.title()), fixed && settings.weather == weather, Action::Weather(weather))).collect();
        chips.push((t("Seasonal"), settings.seasonal && !settings.local_weather, Action::Seasonal));
        for (i, (name, chosen, action)) in chips.into_iter().enumerate() {
            let x = left + (i % 3) as f32 * (width + 8.0);
            let top = *y + (i / 3) as f32 * 42.0;
            self.chip(D2D_RECT_F { left: x, top, right: x + width, bottom: top + 34.0 }, &name, chosen, Hit::App(action))?;
        }
        *y += 2.0 * 42.0;
        let local = D2D_RECT_F { left, top: *y, right, bottom: *y + 34.0 };
        self.chip(local, &t("Match Local Weather"), settings.local_weather, Hit::App(Action::LocalWeather))?;
        *y += 42.0;
        self.label(&view.status, D2D_RECT_F { left, top: *y, right, bottom: *y + 20.0 }, 12.0, false, self.colors.muted, LEADING)?;
        *y += 32.0;
        let amounts = [Snowfall::Light, Snowfall::Steady, Snowfall::Heavy, Snowfall::Variable];
        let names: Vec<String> = amounts.iter().map(|amount| t(amount.title())).collect();
        let chosen = (!settings.local_weather).then(|| amounts.iter().position(|&a| a == settings.snowfall)).flatten();
        self.segmented("Amount", &names, chosen, |i| Action::Snowfall(amounts[i]), left, right, y)?;
        *y += 6.0;
        self.button(D2D_RECT_F { left, top: *y, right, bottom: *y + 36.0 }, &t("Clear Snow"), Hit::App(Action::Clear))
    }

    fn look_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        let names = |options: &[(&str, f32)]| options.iter().map(|(name, _)| t(name)).collect::<Vec<_>>();
        self.segmented("Flake Size", &names(&SIZES), Some(settings.size), Action::Size, left, right, y)?;
        self.segmented("Wind", &names(&WINDS), Some(settings.wind), Action::Wind, left, right, y)?;
        self.segmented("Mouse Wind", &names(&MOUSE_WINDS), Some(settings.mouse_wind), Action::MouseWind, left, right, y)?;
        self.segmented("Opacity", &names(&OPACITIES), Some(settings.opacity), Action::Opacity, left, right, y)?;
        self.segmented("Glitter", &names(&GLITTERS), Some(settings.glitter), Action::Glitter, left, right, y)?;
        self.heading("Color", left, right, y)?;
        let diameter = 28.0;
        let gap = (right - left - COLORS.len() as f32 * diameter) / (COLORS.len() as f32 - 1.0);
        for (i, (_, color)) in COLORS.iter().enumerate() {
            let x = left + i as f32 * (diameter + gap);
            let rect = D2D_RECT_F { left: x, top: *y, right: x + diameter, bottom: *y + diameter };
            let rgb = color.unwrap_or([0.96, 0.97, 1.0]);
            if i == settings.color {
                let ring = D2D_RECT_F { left: x - 4.0, top: *y - 4.0, right: x + diameter + 4.0, bottom: *y + diameter + 4.0 };
                self.outline(ring, diameter, self.colors.accent, 1.0)?;
            }
            self.fill(rect, diameter / 2.0, rgb, 1.0)?;
            self.outline(rect, diameter / 2.0, self.colors.muted, 0.35)?;
            self.hit(rect, Hit::App(Action::Color(i)), diameter / 2.0)?;
        }
        *y += diameter + 10.0;
        Ok(())
    }

    fn behavior_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        // Rain and stars run off: there is nothing for them to pile up or melt.
        let settles = !matches!(view.weather, Weather::Rain | Weather::Stars);
        let rows = [
            ("Accumulate", settings.accumulate, settles, Action::Accumulate),
            ("Melt Snow", settings.melt, settles && settings.accumulate, Action::Melt),
            ("Snow on Windows", settings.snow_on_windows, !settings.behind_windows, Action::SnowOnWindows),
            ("Behind Windows", settings.behind_windows, true, Action::BehindWindows),
            ("Show in Screenshots", settings.show_in_screenshots, true, Action::Screenshots),
            ("Fade While Typing", settings.fade_while_typing, true, Action::FadeWhileTyping),
            ("Open at Login", view.login, true, Action::OpenAtLogin),
        ];
        for (name, on, enabled, action) in rows {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
            let ink = if enabled { self.colors.ink } else { self.colors.muted };
            self.label(&t(name), D2D_RECT_F { right: right - 50.0, ..row }, 13.0, false, ink, LEADING)?;
            let switch = D2D_RECT_F { left: right - 40.0, top: row.top + 7.0, right, bottom: row.top + 25.0 };
            self.switch(switch, on, enabled, Hit::App(action))?;
            *y += 34.0;
        }
        *y += 6.0;
        let schedules: Vec<String> = SCHEDULES.iter().map(|(name, _, _)| t(name)).collect();
        self.segmented("Schedule", &schedules, Some(settings.schedule), Action::Schedule, left, right, y)?;
        let caps: Vec<String> = FRAME_CAPS.iter().map(|(name, _)| t(name)).collect();
        self.segmented("Frame Rate", &caps, Some(settings.frame_cap), Action::FrameCap, left, right, y)
    }

    fn more_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32, capturing: Option<usize>) -> Result<()> {
        self.heading("Snow-Free Programs", left, right, y)?;
        match &view.last_app {
            Some(app) => {
                let rect = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
                self.button(rect, &fill(&t("+ Keep Snow Off {}"), &[app]), Hit::App(Action::Exclude))?;
            }
            None => {
                let rect = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
                self.label(&t("Switch to a program, then open this again"), rect, 12.0, false, self.colors.muted, LEADING)?;
            }
        }
        *y += 38.0;
        for (i, app) in view.excluded.iter().enumerate().take(3) {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 26.0 };
            self.label(app, D2D_RECT_F { right: right - 34.0, ..row }, 12.5, false, self.colors.ink, LEADING)?;
            let remove = D2D_RECT_F { left: right - 26.0, top: row.top, right, bottom: row.bottom };
            self.hit(remove, Hit::App(Action::Include(i)), 6.0)?;
            self.label("✕", remove, 12.0, false, self.colors.muted, CENTER)?;
            *y += 28.0;
        }
        if view.excluded.len() > 3 {
            let more = fill(&t("and {} more"), &[&(view.excluded.len() - 3).to_string()]);
            self.label(&more, D2D_RECT_F { left, top: *y, right, bottom: *y + 20.0 }, 12.0, false, self.colors.muted, LEADING)?;
            *y += 22.0;
        }
        *y += 8.0;

        self.heading("Keyboard Shortcuts", left, right, y)?;
        for (i, (name, _, _)) in SHORTCUTS.iter().enumerate() {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
            self.label(&t(name), D2D_RECT_F { right: right - 150.0, ..row }, 13.0, false, self.colors.ink, LEADING)?;
            let keys = D2D_RECT_F { left: right - 140.0, top: row.top + 2.0, right, bottom: row.bottom - 2.0 };
            let (text, ink) = if capturing == Some(i) {
                (t("Press keys…"), self.colors.accent)
            } else if let Some(hotkey) = view.settings.hotkeys[i] {
                let text = describe(hotkey);
                if view.taken[i] { (fill(&t("{} (taken)"), &[&text]), self.colors.warning) } else { (text, self.colors.ink) }
            } else {
                (t("None"), self.colors.muted)
            };
            self.fill(keys, 7.0, self.colors.surface, 1.0)?;
            if capturing == Some(i) {
                self.outline(keys, 7.0, self.colors.accent, 1.0)?;
            }
            self.hit(keys, Hit::Capture(i), 7.0)?;
            self.label(&text, keys, 12.0, false, ink, CENTER)?;
            *y += 36.0;
        }
        let hint = D2D_RECT_F { left, top: *y, right, bottom: *y + 18.0 };
        self.label(&t("Click a shortcut, then press the new keys. Delete removes it."), hint, 11.5, false, self.colors.muted, LEADING)?;
        *y += 30.0;
        self.button(D2D_RECT_F { left, top: *y, right, bottom: *y + 36.0 }, &t("Check for Updates…"), Hit::App(Action::Updates))
    }

    fn heading(&mut self, text: &str, left: f32, right: f32, y: &mut f32) -> Result<()> {
        self.label(&t(text), D2D_RECT_F { left, top: *y, right, bottom: *y + 18.0 }, 12.0, true, self.colors.muted, LEADING)?;
        *y += 24.0;
        Ok(())
    }

    /// A row of options of which one is chosen, under its label.
    fn segmented(
        &mut self,
        title: &str,
        options: &[String],
        chosen: Option<usize>,
        action: impl Fn(usize) -> Action,
        left: f32,
        right: f32,
        y: &mut f32,
    ) -> Result<()> {
        self.heading(title, left, right, y)?;
        let bar = D2D_RECT_F { left, top: *y, right, bottom: *y + 30.0 };
        self.fill(bar, 8.0, self.colors.surface, 1.0)?;
        let width = (right - left - 4.0) / options.len() as f32;
        for (i, option) in options.iter().enumerate() {
            let x = left + 2.0 + i as f32 * width;
            let rect = D2D_RECT_F { left: x, top: *y + 2.0, right: x + width, bottom: *y + 28.0 };
            let selected = chosen == Some(i);
            if selected {
                self.fill(rect, 6.0, self.colors.accent, 1.0)?;
            }
            self.hit(rect, Hit::App(action(i)), 6.0)?;
            let ink = if selected { self.colors.on_accent } else { self.colors.ink };
            self.label(option, rect, 12.0, selected, ink, CENTER)?;
        }
        *y += 42.0;
        Ok(())
    }

    fn chip(&mut self, rect: D2D_RECT_F, text: &str, chosen: bool, hit: Hit) -> Result<()> {
        if chosen {
            self.fill(rect, 9.0, self.colors.accent, 1.0)?;
        } else {
            self.fill(rect, 9.0, self.colors.surface, 1.0)?;
        }
        self.hit(rect, hit, 9.0)?;
        let ink = if chosen { self.colors.on_accent } else { self.colors.ink };
        self.label(text, rect, 12.5, chosen, ink, CENTER)
    }

    fn button(&mut self, rect: D2D_RECT_F, text: &str, hit: Hit) -> Result<()> {
        self.fill(rect, 9.0, self.colors.surface, 1.0)?;
        self.hit(rect, hit, 9.0)?;
        self.label(text, rect, 12.5, false, self.colors.ink, CENTER)
    }

    fn switch(&mut self, rect: D2D_RECT_F, on: bool, enabled: bool, hit: Hit) -> Result<()> {
        let radius = (rect.bottom - rect.top) / 2.0;
        let alpha = if enabled { 1.0 } else { 0.4 };
        if on {
            self.fill(rect, radius, self.colors.accent, alpha)?;
        } else {
            self.fill(rect, radius, self.colors.raised, alpha)?;
            self.outline(rect, radius, self.colors.muted, 0.5 * alpha)?;
        }
        let knob = radius - 4.0;
        let x = if on { rect.right - radius } else { rect.left + radius };
        let ink = if on { self.colors.on_accent } else { self.colors.muted };
        unsafe {
            let brush = self.target.CreateSolidColorBrush(&color(ink, alpha), None)?;
            let ellipse = D2D1_ELLIPSE { point: Vector2 { X: x, Y: rect.top + radius }, radiusX: knob, radiusY: knob };
            self.target.FillEllipse(&ellipse, &brush);
        }
        if enabled {
            self.hit(rect, hit, radius)?;
        }
        Ok(())
    }

    /// Records a control, with a light wash while the pointer is over it.
    fn hit(&mut self, rect: D2D_RECT_F, hit: Hit, radius: f32) -> Result<()> {
        if self.hover == Some(self.hits.len()) {
            self.fill(rect, radius, self.colors.ink, 0.07)?;
        }
        self.hits.push((rect, hit));
        Ok(())
    }

    fn fill(&self, rect: D2D_RECT_F, radius: f32, rgb: [f32; 3], alpha: f32) -> Result<()> {
        unsafe {
            let brush = self.target.CreateSolidColorBrush(&color(rgb, alpha), None)?;
            self.target.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect, radiusX: radius, radiusY: radius }, &brush);
        }
        Ok(())
    }

    fn outline(&self, rect: D2D_RECT_F, radius: f32, rgb: [f32; 3], alpha: f32) -> Result<()> {
        if alpha <= 0.0 {
            return Ok(());
        }
        let inset = D2D_RECT_F { left: rect.left + 0.5, top: rect.top + 0.5, right: rect.right - 0.5, bottom: rect.bottom - 0.5 };
        unsafe {
            let brush = self.target.CreateSolidColorBrush(&color(rgb, alpha), None)?;
            self.target.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: inset, radiusX: radius, radiusY: radius }, &brush, 1.0, None);
        }
        Ok(())
    }

    /// One line of text, centred vertically in `rect`.
    fn label(&self, text: &str, rect: D2D_RECT_F, size: f32, bold: bool, rgb: [f32; 3], align: DWRITE_TEXT_ALIGNMENT) -> Result<()> {
        let weight: DWRITE_FONT_WEIGHT = if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let units: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let format = self.text.CreateTextFormat(w!("Segoe UI"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))?;
            format.SetTextAlignment(align)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let brush = self.target.CreateSolidColorBrush(&color(rgb, 1.0), None)?;
            let inner = D2D_RECT_F { left: rect.left + 6.0 * (align != LEADING) as u8 as f32, right: rect.right - 6.0 * (align != LEADING) as u8 as f32, ..rect };
            self.target.DrawText(&units, &format, &inner, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, Default::default());
        }
        Ok(())
    }
}

const LEADING: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_LEADING;
const CENTER: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_CENTER;
const TRAILING: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_TRAILING;

fn color(rgb: [f32; 3], alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb[0], g: rgb[1], b: rgb[2], a: alpha }
}
