//! The settings panel that opens from the tray icon: black and white over frosted glass, drawn with Direct2D.
//! On Windows 11 the glass is the system's acrylic backdrop and the corners are the system's; elsewhere the panel is
//! solid. Everything is laid out again on each paint, which also records where each control is, so a click finds its
//! action.

use std::cell::{Cell, RefCell};
use std::mem::size_of;

use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, GetKeyState, MAPVK_VK_TO_VSC, MapVirtualKeyW, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_BACK, VK_CONTROL, VK_DELETE,
    VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, Result, w};
use windows_numerics::Vector2;

use snowhush_core::simulation::{MAX_COLORS, Snowfall, Weather};

use crate::log;
use crate::prefs::{FRAME_CAPS, GLITTERS, Hotkey, MOUSE_WINDS, OPACITIES, SCHEDULES, SHORTCUTS, SIZES, Settings, WINDS};
use crate::text::{fill, t};
use crate::tracker::windows_build;

const CLASS: PCWSTR = w!("DesktopFloraPanel");
const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 600.0;
const PAD: f32 = 20.0;
const TABS: [&str; 4] = ["Weather", "Look", "Behavior", "More"];
const WM_MOUSELEAVE: u32 = 0x02A3;
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;
/// The first Windows 11 build with acrylic backdrops for any window.
const ACRYLIC_BUILD: u32 = 22621;

/// What a click on the panel asks for.
#[derive(Clone, Debug)]
pub enum Action {
    Toggle,
    Snowfall(Snowfall),
    Weather(Weather),
    /// Where the weather comes from: chosen here, the season, or the sky outside.
    Source(Source),
    Size(usize),
    Wind(usize),
    MouseWind(usize),
    CustomColors(bool),
    AddColor([f32; 3]),
    RemoveColor(usize),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Chosen,
    Season,
    Outside,
}

/// What the panel shows, read from the app each time it is drawn.
pub struct View {
    pub on: bool,
    pub settings: Settings,
    /// What falls now and how much, whichever way it was chosen.
    pub weather: Weather,
    pub amount: Snowfall,
    /// Two lines on where the weather comes from, as the season, the sky outside and the place.
    pub status: (String, String),
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

/// What a panel does with a click: the app's actions, and its own tabs and shortcut fields.
#[derive(Clone, Debug)]
enum Hit {
    App(Action),
    Tab(usize),
    Capture(usize),
}

struct Panel {
    window: HWND,
    scale: f32,
    glass: bool,
    tab: usize,
    hover: Option<usize>,
    hits: Vec<(D2D_RECT_F, Hit)>,
    /// The shortcut waiting for its new keys.
    capturing: Option<usize>,
    handlers: Handlers,
    d2d: ID2D1Factory,
    text: IDWriteFactory,
    target: Option<ID2D1HwndRenderTarget>,
}

thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    /// The tab to open on next time, as the panel closes each time it loses focus.
    static LAST_TAB: Cell<usize> = const { Cell::new(0) };
    /// When the panel last closed: a click on the tray icon closes it by taking the focus, and shouldn't open it again.
    static CLOSED_AT: Cell<u32> = const { Cell::new(0) };
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
    let (scale, origin) = placement();
    let size = ((WIDTH * scale).round() as i32, (HEIGHT * scale).round() as i32);
    let dark = !light_theme();
    let glass = windows_build() >= ACRYLIC_BUILD;
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            w!("DesktopFlora"),
            WS_POPUP,
            origin.x,
            origin.y,
            size.0,
            size.1,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let attribute = |kind, value: u32| {
            let _ = DwmSetWindowAttribute(window, kind, (&raw const value).cast(), 4);
        };
        attribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWM_WINDOW_CORNER_PREFERENCE(DWMWCP_ROUND.0).0 as u32);
        attribute(DWMWA_USE_IMMERSIVE_DARK_MODE, BOOL::from(dark).0 as u32);
        if glass {
            // The acrylic shows wherever the panel draws less than opaque: the whole window is frame to the system.
            let _ = DwmExtendFrameIntoClientArea(window, &MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 });
            attribute(DWMWA_SYSTEMBACKDROP_TYPE, DWM_SYSTEMBACKDROP_TYPE(DWMSBT_TRANSIENTWINDOW.0).0 as u32);
        }
        PANEL.set(Some(Panel {
            window,
            scale,
            glass,
            tab: LAST_TAB.get(),
            hover: None,
            hits: Vec::new(),
            capturing: None,
            handlers,
            d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
            text: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
            target: None,
        }));
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
        paint();
    }
    Ok(())
}

/// The display's scale, and where the panel goes: next to the tray, on whichever side the taskbar is.
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
        let (width, height, gap) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32, (12.0 * scale) as i32);
        let RECT { left, top, right, bottom } = info.rcWork;
        let x = (cursor.x - width / 2).clamp(left + gap, (right - gap - width).max(left + gap));
        let y = if cursor.y >= bottom {
            bottom - gap - height
        } else if cursor.y < top {
            top + gap
        } else {
            (cursor.y - height / 2).clamp(top + gap, (bottom - gap - height).max(top + gap))
        };
        (scale, POINT { x, y })
    }
}

fn paint() {
    let result = PANEL.with_borrow_mut(|panel| {
        let panel = panel.as_mut()?;
        let view = (panel.handlers.view)()?;
        Some(panel.render(&view))
    });
    if let Some(Err(error)) = result {
        log!("panel paint failed: {error}");
        // A lost device takes the render target with it; the next paint makes a new one.
        PANEL.with_borrow_mut(|panel| {
            if let Some(panel) = panel {
                panel.target = None;
            }
        });
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => {
            close();
            LRESULT(0)
        }
        WM_PAINT => {
            paint();
            unsafe {
                let _ = ValidateRect(Some(window), None);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let point = point(lparam);
            let changed = PANEL.with_borrow_mut(|panel| {
                let panel = panel.as_mut()?;
                let at = (point.x as f32 / panel.scale, point.y as f32 / panel.scale);
                let hover = panel.hits.iter().position(|(rect, _)| contains(rect, at));
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
            let point = point(lparam);
            let hit = PANEL.with_borrow(|panel| {
                let panel = panel.as_ref()?;
                let at = (point.x as f32 / panel.scale, point.y as f32 / panel.scale);
                panel.hits.iter().find(|(rect, _)| contains(rect, at)).map(|(rect, hit)| match hit {
                    // The colour strip gives the colour under the pointer.
                    Hit::App(Action::AddColor(_)) => Hit::App(Action::AddColor(picked(rect, at))),
                    _ => hit.clone(),
                })
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

fn point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 }
}

fn contains(rect: &D2D_RECT_F, (x, y): (f32, f32)) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn click(hit: Hit) {
    match hit {
        Hit::Tab(tab) => {
            let was_capturing = PANEL.with_borrow_mut(|panel| {
                let panel = panel.as_mut()?;
                panel.tab = tab;
                panel.capturing.take()
            });
            if was_capturing.is_some() {
                suspend(false);
            }
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

fn suspend(on: bool) {
    let act = take_act();
    if let Some(act) = &act {
        act(Action::SuspendShortcuts(on));
    }
    put_act(act);
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

fn light_theme() -> bool {
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
    status == ERROR_SUCCESS && light != 0
}

/// Black and white: ink and its accents are the theme's foreground, surfaces are faint washes of it over the glass.
struct Palette {
    /// The panel's own tint over the glass; nearly opaque without glass.
    base: [f32; 4],
    ink: [f32; 3],
    muted: f32,
    wash: f32,
    raised: f32,
    on_accent: [f32; 3],
    warning: [f32; 3],
}

fn palette(glass: bool) -> Palette {
    if light_theme() {
        Palette {
            base: [0.98, 0.98, 0.98, if glass { 0.55 } else { 0.98 }],
            ink: [0.06, 0.06, 0.07],
            muted: 0.55,
            wash: 0.06,
            raised: 0.12,
            on_accent: [1.0; 3],
            warning: [0.78, 0.2, 0.16],
        }
    } else {
        Palette {
            base: [0.05, 0.05, 0.055, if glass { 0.5 } else { 0.96 }],
            ink: [0.97, 0.97, 0.98],
            muted: 0.55,
            wash: 0.07,
            raised: 0.15,
            on_accent: [0.04, 0.04, 0.05],
            warning: [1.0, 0.5, 0.45],
        }
    }
}

impl Panel {
    fn render(&mut self, view: &View) -> Result<()> {
        if self.target.is_none() {
            let mut client = RECT::default();
            unsafe {
                let _ = GetClientRect(self.window, &mut client);
                let properties = D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                    dpiX: 96.0 * self.scale,
                    dpiY: 96.0 * self.scale,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                };
                let window = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd: self.window,
                    pixelSize: D2D_SIZE_U { width: client.right as u32, height: client.bottom as u32 },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                };
                self.target = Some(self.d2d.CreateHwndRenderTarget(&properties, &window)?);
            }
        }
        let Some(target) = &self.target else { return Ok(()) };
        unsafe {
            target.BeginDraw();
            target.Clear(Some(&color([0.0; 3], 0.0)));
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        }
        let mut painter = Painter { target, text: &self.text, colors: palette(self.glass), hits: Vec::new(), hover: self.hover };
        let drawn = painter.panel(view, self.tab, self.capturing);
        self.hits = std::mem::take(&mut painter.hits);
        unsafe { target.EndDraw(None, None)? };
        drawn
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
        let frame = D2D_RECT_F { left: 0.0, top: 0.0, right: WIDTH, bottom: HEIGHT };
        let [r, g, b, a] = self.colors.base;
        self.fill(frame, 0.0, [r, g, b], a)?;
        let (left, right) = (frame.left + PAD, frame.right - PAD);

        // Header: name, what falls now, and the switch.
        self.label("DesktopFlora", D2D_RECT_F { left, top: 16.0, right, bottom: 40.0 }, 17.0, true, self.colors.ink, 1.0, LEADING)?;
        let falling = if view.on { fill(&t("{} · {}"), &[&t(view.weather.title()), &t(view.amount.title())]) } else { t("Paused") };
        self.label(&falling, D2D_RECT_F { left, top: 40.0, right: right - 60.0, bottom: 58.0 }, 12.0, false, self.colors.ink, self.colors.muted, LEADING)?;
        self.switch(D2D_RECT_F { left: right - 46.0, top: 24.0, right, bottom: 48.0 }, view.on, true, Hit::App(Action::Toggle))?;

        // Tabs: a quiet bar with the chosen one lifted out of it.
        let top = 74.0;
        let tab_width = (right - left - 4.0) / TABS.len() as f32;
        self.fill(D2D_RECT_F { left, top, right, bottom: top + 32.0 }, 9.0, self.colors.ink, self.colors.wash)?;
        for (i, name) in TABS.iter().enumerate() {
            let x = left + 2.0 + i as f32 * tab_width;
            let rect = D2D_RECT_F { left: x, top: top + 2.0, right: x + tab_width, bottom: top + 30.0 };
            let chosen = i == tab;
            if chosen {
                self.fill(rect, 7.0, self.colors.ink, self.colors.raised)?;
            }
            self.hit(rect, Hit::Tab(i), 7.0)?;
            self.label(&t(name), rect, 12.0, chosen, self.colors.ink, if chosen { 1.0 } else { self.colors.muted }, CENTER)?;
        }

        let mut y = top + 48.0;
        match tab {
            0 => self.weather_tab(view, left, right, &mut y)?,
            1 => self.look_tab(view, left, right, &mut y)?,
            2 => self.behavior_tab(view, left, right, &mut y)?,
            _ => self.more_tab(view, left, right, &mut y, capturing)?,
        }

        // Footer.
        let bottom = HEIGHT - 12.0;
        self.fill(D2D_RECT_F { left, top: bottom - 32.0, right, bottom: bottom - 31.0 }, 0.0, self.colors.ink, 0.1)?;
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        self.label(&version, D2D_RECT_F { left, top: bottom - 26.0, right: right - 100.0, bottom }, 11.5, false, self.colors.ink, 0.45, LEADING)?;
        let quit = D2D_RECT_F { left: right - 90.0, top: bottom - 28.0, right, bottom };
        self.hit(quit, Hit::App(Action::Quit), 6.0)?;
        self.label(&t("Quit"), quit, 12.0, false, self.colors.ink, self.colors.muted, TRAILING)
    }

    fn weather_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        let source = if settings.local_weather {
            Source::Outside
        } else if settings.seasonal {
            Source::Season
        } else {
            Source::Chosen
        };
        let sources = [Source::Chosen, Source::Season, Source::Outside];
        let names: Vec<String> = ["Choose", "Season", "Outside"].iter().map(|name| t(name)).collect();
        let chosen = sources.iter().position(|&s| s == source);
        self.segmented("What falls", &names, chosen, |i| Action::Source(sources[i]), left, right, y)?;

        if source == Source::Chosen {
            // One tile per weather, with its sign above its name.
            let count = Weather::ALL.len() as f32;
            let width = (right - left - (count - 1.0) * 8.0) / count;
            for (i, weather) in Weather::ALL.iter().enumerate() {
                let x = left + i as f32 * (width + 8.0);
                let rect = D2D_RECT_F { left: x, top: *y, right: x + width, bottom: *y + 66.0 };
                let selected = settings.weather == *weather;
                self.fill(rect, 10.0, self.colors.ink, if selected { 1.0 } else { self.colors.wash })?;
                self.hit(rect, Hit::App(Action::Weather(*weather)), 10.0)?;
                let ink = if selected { self.colors.on_accent } else { self.colors.ink };
                let sign = D2D_RECT_F { top: rect.top + 8.0, bottom: rect.top + 38.0, ..rect };
                self.glyph(sign_of(*weather), sign, 20.0, ink)?;
                let name = D2D_RECT_F { top: rect.top + 40.0, bottom: rect.bottom - 6.0, ..rect };
                self.label(&t(weather.title()), name, 11.0, selected, ink, 1.0, CENTER)?;
            }
            *y += 78.0;
        } else {
            // Where it comes from: what falls, the season or sky, and the place.
            let card = D2D_RECT_F { left, top: *y, right, bottom: *y + 66.0 };
            self.fill(card, 10.0, self.colors.ink, self.colors.wash)?;
            self.glyph(sign_of(view.weather), D2D_RECT_F { left: left + 8.0, right: left + 50.0, ..card }, 22.0, self.colors.ink)?;
            let text = D2D_RECT_F { left: left + 56.0, right: right - 12.0, ..card };
            self.label(&view.status.0, D2D_RECT_F { top: card.top + 12.0, bottom: card.top + 34.0, ..text }, 13.0, true, self.colors.ink, 1.0, LEADING)?;
            self.label(&view.status.1, D2D_RECT_F { top: card.top + 34.0, bottom: card.top + 54.0, ..text }, 11.5, false, self.colors.ink, self.colors.muted, LEADING)?;
            *y += 78.0;
        }

        let amounts = [Snowfall::Light, Snowfall::Steady, Snowfall::Heavy, Snowfall::Variable];
        let names: Vec<String> = amounts.iter().map(|amount| t(amount.title())).collect();
        let chosen = (source != Source::Outside).then(|| amounts.iter().position(|&a| a == settings.snowfall)).flatten();
        self.segmented("Amount", &names, chosen, |i| Action::Snowfall(amounts[i]), left, right, y)?;
        *y += 4.0;
        self.button(D2D_RECT_F { left, top: *y, right, bottom: *y + 36.0 }, &t("Clear Snow"), Hit::App(Action::Clear))
    }

    fn look_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        let names = |options: &[(&str, f32)]| options.iter().map(|(name, _)| t(name)).collect::<Vec<_>>();
        self.segmented("Size", &names(&SIZES), Some(settings.size), Action::Size, left, right, y)?;
        self.segmented("Wind", &names(&WINDS), Some(settings.wind), Action::Wind, left, right, y)?;
        self.segmented("Mouse Wind", &names(&MOUSE_WINDS), Some(settings.mouse_wind), Action::MouseWind, left, right, y)?;
        self.segmented("Opacity", &names(&OPACITIES), Some(settings.opacity), Action::Opacity, left, right, y)?;
        self.segmented("Glitter", &names(&GLITTERS), Some(settings.glitter), Action::Glitter, left, right, y)?;
        let kinds = [t("Natural"), t("My Colors")];
        self.segmented("Color", &kinds, Some(settings.custom_colors as usize), |i| Action::CustomColors(i == 1), left, right, y)?;
        if !settings.custom_colors {
            return Ok(());
        }
        // The chosen colours: a click removes one.
        let diameter = 24.0;
        let count = settings.palette_len.min(MAX_COLORS);
        for (i, rgb) in settings.palette[..count].iter().enumerate() {
            let x = left + i as f32 * (diameter + 10.0);
            let rect = D2D_RECT_F { left: x, top: *y, right: x + diameter, bottom: *y + diameter };
            self.fill(rect, diameter / 2.0, *rgb, 1.0)?;
            self.outline(rect, diameter / 2.0, self.colors.ink, 0.25)?;
            let hovered = self.hover == Some(self.hits.len());
            self.hits.push((rect, Hit::App(Action::RemoveColor(i))));
            if hovered {
                let ink = if rgb.iter().sum::<f32>() > 1.6 { [0.0; 3] } else { [1.0; 3] };
                self.label("✕", rect, 11.0, true, ink, 1.0, CENTER)?;
            }
        }
        let hint = if count == 0 {
            t("Pick colors below")
        } else if count < MAX_COLORS {
            t("Click one to remove it")
        } else {
            t("Full: remove one to add another")
        };
        let after = left + count as f32 * (diameter + 10.0);
        self.label(&hint, D2D_RECT_F { left: after, top: *y, right, bottom: *y + diameter }, 11.5, false, self.colors.ink, self.colors.muted, LEADING)?;
        *y += diameter + 10.0;
        // A strip of colours to pick from: vivid, pastel and greys.
        let strip = D2D_RECT_F { left, top: *y, right, bottom: *y + 42.0 };
        let steps = 48;
        let step = (right - left) / steps as f32;
        for row in 0..3 {
            for i in 0..steps {
                let x = left + i as f32 * step;
                let rect = D2D_RECT_F { left: x, top: *y + row as f32 * 14.0, right: x + step + 0.5, bottom: *y + (row + 1) as f32 * 14.0 };
                self.fill(rect, 0.0, strip_color(row, i as f32 / (steps - 1) as f32), 1.0)?;
            }
        }
        self.outline(strip, 6.0, self.colors.ink, 0.15)?;
        if count < MAX_COLORS {
            self.hits.push((strip, Hit::App(Action::AddColor([1.0; 3]))));
        }
        *y += 50.0;
        Ok(())
    }

    fn behavior_tab(&mut self, view: &View, left: f32, right: f32, y: &mut f32) -> Result<()> {
        let settings = &view.settings;
        // Rain and stars run off: there is nothing for them to pile up or melt.
        let settles = !matches!(view.weather, Weather::Rain | Weather::Stars);
        let rows = [
            ("Open at Login", view.login, true, Action::OpenAtLogin),
            ("Accumulate", settings.accumulate, settles, Action::Accumulate),
            // Only snow melts; leaves and petals stay.
            ("Melt Snow", settings.melt, view.weather == Weather::Snow && settings.accumulate, Action::Melt),
            ("Snow on Windows", settings.snow_on_windows, !settings.behind_windows, Action::SnowOnWindows),
            ("Behind Windows", settings.behind_windows, true, Action::BehindWindows),
            ("Show in Screenshots", settings.show_in_screenshots, true, Action::Screenshots),
            ("Fade While Typing", settings.fade_while_typing, true, Action::FadeWhileTyping),
        ];
        for (name, on, enabled, action) in rows {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
            let alpha = if enabled { 1.0 } else { 0.4 };
            self.label(&t(name), D2D_RECT_F { right: right - 50.0, ..row }, 13.0, false, self.colors.ink, alpha, LEADING)?;
            let switch = D2D_RECT_F { left: right - 38.0, top: row.top + 7.0, right, bottom: row.top + 25.0 };
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
        let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 34.0 };
        match &view.last_app {
            Some(app) => self.button(row, &fill(&t("+ Keep Snow Off {}"), &[app]), Hit::App(Action::Exclude))?,
            None => self.label(&t("Switch to a program, then open this again"), row, 12.0, false, self.colors.ink, self.colors.muted, LEADING)?,
        }
        *y += 42.0;
        for (i, app) in view.excluded.iter().enumerate().take(3) {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 26.0 };
            self.label(app, D2D_RECT_F { right: right - 34.0, ..row }, 12.5, false, self.colors.ink, 1.0, LEADING)?;
            let remove = D2D_RECT_F { left: right - 26.0, top: row.top, right, bottom: row.bottom };
            self.hit(remove, Hit::App(Action::Include(i)), 6.0)?;
            self.label("✕", remove, 11.0, false, self.colors.ink, self.colors.muted, CENTER)?;
            *y += 28.0;
        }
        if view.excluded.len() > 3 {
            let more = fill(&t("and {} more"), &[&(view.excluded.len() - 3).to_string()]);
            self.label(&more, D2D_RECT_F { left, top: *y, right, bottom: *y + 20.0 }, 12.0, false, self.colors.ink, self.colors.muted, LEADING)?;
            *y += 22.0;
        }
        *y += 10.0;

        self.heading("Keyboard Shortcuts", left, right, y)?;
        for (i, (name, _, _)) in SHORTCUTS.iter().enumerate() {
            let row = D2D_RECT_F { left, top: *y, right, bottom: *y + 32.0 };
            self.label(&t(name), D2D_RECT_F { right: right - 150.0, ..row }, 13.0, false, self.colors.ink, 1.0, LEADING)?;
            let keys = D2D_RECT_F { left: right - 140.0, top: row.top + 2.0, right, bottom: row.bottom - 2.0 };
            let (text, ink, alpha) = if capturing == Some(i) {
                (t("Press keys…"), self.colors.ink, 1.0)
            } else if let Some(hotkey) = view.settings.hotkeys[i] {
                let text = describe(hotkey);
                if view.taken[i] { (fill(&t("{} (taken)"), &[&text]), self.colors.warning, 1.0) } else { (text, self.colors.ink, 1.0) }
            } else {
                (t("None"), self.colors.ink, self.colors.muted)
            };
            self.fill(keys, 7.0, self.colors.ink, self.colors.wash)?;
            if capturing == Some(i) {
                self.outline(keys, 7.0, self.colors.ink, 0.9)?;
            }
            self.hit(keys, Hit::Capture(i), 7.0)?;
            self.label(&text, keys, 12.0, false, ink, alpha, CENTER)?;
            *y += 36.0;
        }
        let hint = D2D_RECT_F { left, top: *y, right, bottom: *y + 18.0 };
        self.label(&t("Click a shortcut, then press the new keys. Delete removes it."), hint, 11.0, false, self.colors.ink, self.colors.muted, LEADING)?;
        *y += 30.0;
        self.button(D2D_RECT_F { left, top: *y, right, bottom: *y + 36.0 }, &t("Check for Updates…"), Hit::App(Action::Updates))
    }

    fn heading(&mut self, text: &str, left: f32, right: f32, y: &mut f32) -> Result<()> {
        self.label(&t(text), D2D_RECT_F { left, top: *y, right, bottom: *y + 16.0 }, 11.5, true, self.colors.ink, self.colors.muted, LEADING)?;
        *y += 22.0;
        Ok(())
    }

    /// A row of options of which one is chosen, under its heading.
    #[allow(clippy::too_many_arguments)]
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
        self.fill(D2D_RECT_F { left, top: *y, right, bottom: *y + 30.0 }, 8.0, self.colors.ink, self.colors.wash)?;
        let width = (right - left - 4.0) / options.len() as f32;
        for (i, option) in options.iter().enumerate() {
            let x = left + 2.0 + i as f32 * width;
            let rect = D2D_RECT_F { left: x, top: *y + 2.0, right: x + width, bottom: *y + 28.0 };
            let selected = chosen == Some(i);
            if selected {
                self.fill(rect, 6.0, self.colors.ink, 1.0)?;
            }
            self.hit(rect, Hit::App(action(i)), 6.0)?;
            let ink = if selected { self.colors.on_accent } else { self.colors.ink };
            self.label(option, rect, 11.5, selected, ink, 1.0, CENTER)?;
        }
        *y += 40.0;
        Ok(())
    }

    fn button(&mut self, rect: D2D_RECT_F, text: &str, hit: Hit) -> Result<()> {
        self.fill(rect, 9.0, self.colors.ink, self.colors.wash)?;
        self.outline(rect, 9.0, self.colors.ink, 0.12)?;
        self.hit(rect, hit, 9.0)?;
        self.label(text, rect, 12.0, false, self.colors.ink, 1.0, CENTER)
    }

    fn switch(&mut self, rect: D2D_RECT_F, on: bool, enabled: bool, hit: Hit) -> Result<()> {
        let radius = (rect.bottom - rect.top) / 2.0;
        let alpha = if enabled { 1.0 } else { 0.35 };
        if on {
            self.fill(rect, radius, self.colors.ink, alpha)?;
        } else {
            self.fill(rect, radius, self.colors.ink, self.colors.wash)?;
            self.outline(rect, radius, self.colors.ink, 0.35 * alpha)?;
        }
        let knob = radius - 4.0;
        let x = if on { rect.right - radius } else { rect.left + radius };
        let ink = if on { self.colors.on_accent } else { self.colors.ink };
        unsafe {
            let brush = self.target.CreateSolidColorBrush(&color(ink, if on { 1.0 } else { 0.6 * alpha }), None)?;
            let ellipse = D2D1_ELLIPSE { point: Vector2 { X: x, Y: rect.top + radius }, radiusX: knob, radiusY: knob };
            self.target.FillEllipse(&ellipse, &brush);
        }
        if enabled {
            self.hit(rect, hit, radius)?;
        }
        Ok(())
    }

    /// Records a control, with a faint wash while the pointer is over it.
    fn hit(&mut self, rect: D2D_RECT_F, hit: Hit, radius: f32) -> Result<()> {
        if self.hover == Some(self.hits.len()) {
            self.fill(rect, radius, self.colors.ink, 0.08)?;
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
        let inset = D2D_RECT_F { left: rect.left + 0.5, top: rect.top + 0.5, right: rect.right - 0.5, bottom: rect.bottom - 0.5 };
        unsafe {
            let brush = self.target.CreateSolidColorBrush(&color(rgb, alpha), None)?;
            self.target.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: inset, radiusX: radius, radiusY: radius }, &brush, 1.0, None);
        }
        Ok(())
    }

    /// One line of text, centred vertically in `rect`.
    #[allow(clippy::too_many_arguments)]
    fn label(&self, text: &str, rect: D2D_RECT_F, size: f32, bold: bool, rgb: [f32; 3], alpha: f32, align: DWRITE_TEXT_ALIGNMENT) -> Result<()> {
        let weight: DWRITE_FONT_WEIGHT = if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        self.text(text, w!("Segoe UI Variable Text"), rect, size, weight, rgb, alpha, align)
    }

    /// A weather's sign, in the symbol font, which draws them in one colour.
    fn glyph(&self, text: &str, rect: D2D_RECT_F, size: f32, rgb: [f32; 3]) -> Result<()> {
        self.text(text, w!("Segoe UI Symbol"), rect, size, DWRITE_FONT_WEIGHT_NORMAL, rgb, 1.0, CENTER)
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        text: &str,
        family: PCWSTR,
        rect: D2D_RECT_F,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        rgb: [f32; 3],
        alpha: f32,
        align: DWRITE_TEXT_ALIGNMENT,
    ) -> Result<()> {
        let units: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let format = self.text.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))?;
            format.SetTextAlignment(align)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let brush = self.target.CreateSolidColorBrush(&color(rgb, alpha), None)?;
            let inset = if align == LEADING { 0.0 } else { 4.0 };
            let inner = D2D_RECT_F { left: rect.left + inset, right: rect.right - inset, ..rect };
            self.target.DrawText(&units, &format, &inner, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, Default::default());
        }
        Ok(())
    }
}

/// A weather's sign, in one colour.
fn sign_of(weather: Weather) -> &'static str {
    match weather {
        Weather::Snow => "❄",
        Weather::Rain => "☂",
        Weather::Blossoms => "✿",
        Weather::Leaves => "❦",
        Weather::Stars => "✦",
    }
}

/// The colour strip's colour at `t` across row `row`: vivid hues, pastel hues, then white to black.
fn strip_color(row: usize, t: f32) -> [f32; 3] {
    match row {
        0 => hsl(t * 360.0, 0.8, 0.55),
        1 => hsl(t * 360.0, 0.75, 0.82),
        _ => [1.0 - t; 3],
    }
}

/// The strip's colour under the pointer.
fn picked(strip: &D2D_RECT_F, (x, y): (f32, f32)) -> [f32; 3] {
    let t = ((x - strip.left) / (strip.right - strip.left)).clamp(0.0, 1.0);
    let row = (((y - strip.top) / 14.0) as usize).min(2);
    strip_color(row, t)
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = (hue % 360.0) / 60.0;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    [r + m, g + m, b + m]
}

const LEADING: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_LEADING;
const CENTER: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_CENTER;
const TRAILING: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_TRAILING;

fn color(rgb: [f32; 3], alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb[0], g: rgb[1], b: rgb[2], a: alpha }
}
