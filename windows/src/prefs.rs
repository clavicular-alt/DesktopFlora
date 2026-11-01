use std::ffi::c_void;

use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW};
use windows::core::{BOOL, PCWSTR, w};

use snowhush_core::simulation::{Look, MAX_COLORS, Snowfall, Weather};

const KEY: PCWSTR = w!("Software\\Snowhush");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Snowhush");

/// Flake size, wind, opacity and frame rate choices, in menu order.
pub const SIZES: [(&str, f32); 3] = [("Small", 0.7), ("Normal", 1.0), ("Large", 1.45)];
pub const WINDS: [(&str, f32); 3] = [("Still", 0.25), ("Breezy", 1.0), ("Stormy", 3.0)];
pub const OPACITIES: [(&str, f32); 4] = [("100%", 1.0), ("75%", 0.75), ("50%", 0.5), ("25%", 0.25)];
/// Zero lets the display's own rate through.
pub const FRAME_CAPS: [(&str, u32); 4] = [("Auto", 0), ("60 fps", 60), ("30 fps", 30), ("Eco", 20)];
pub const MOUSE_WINDS: [(&str, f32); 3] = [("Off", 0.0), ("Gentle", 1.0), ("Strong", 2.5)];
pub const GLITTERS: [(&str, f32); 3] = [("Off", 0.0), ("Normal", 1.0), ("Sparkly", 3.0)];
/// When the snow shows: hours from and to, local time; the same hour twice means always.
pub const SCHEDULES: [(&str, u32, u32); 4] = [("Always", 0, 0), ("Evenings", 17, 23), ("Daytime", 8, 17), ("Nights", 20, 6)];

/// A keyboard shortcut: `MOD_*` flags as `RegisterHotKey` takes them, and a virtual-key code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub modifiers: u32,
    pub key: u32,
}

const CTRL_ALT: u32 = 0x0002 | 0x0001;
/// What each shortcut does, where it is kept, and what it is until someone changes it.
pub const SHORTCUTS: [(&str, PCWSTR, Hotkey); 3] = [
    ("Snow on/off", w!("hotkeyToggle"), Hotkey { modifiers: CTRL_ALT, key: b'S' as u32 }),
    ("Clear snow", w!("hotkeyClear"), Hotkey { modifiers: CTRL_ALT, key: b'C' as u32 }),
    ("Next weather", w!("hotkeyWeather"), Hotkey { modifiers: CTRL_ALT, key: b'W' as u32 }),
];

#[derive(Clone, Copy)]
pub struct Settings {
    /// Switched off: nothing falls and nothing shows, whatever else is chosen.
    pub paused: bool,
    /// How much falls while on; never `Off`, which is what `paused` is for.
    pub snowfall: Snowfall,
    pub weather: Weather,
    pub size: usize,
    pub wind: usize,
    pub opacity: usize,
    pub frame_cap: usize,
    /// Fades the snow while the keyboard is in use.
    pub fade_while_typing: bool,
    /// Follows the weather outside instead of the chosen snowfall and weather.
    pub local_weather: bool,
    /// Changes the weather with the seasons: snow in winter, blossoms in spring, rain in summer, leaves in autumn.
    pub seasonal: bool,
    pub mouse_wind: usize,
    pub glitter: usize,
    /// Everything falls in the palette's colours instead of its own.
    pub custom_colors: bool,
    pub palette: [[f32; 3]; MAX_COLORS],
    pub palette_len: usize,
    pub schedule: usize,
    /// One per entry of `SHORTCUTS`; `None` when someone removed it.
    pub hotkeys: [Option<Hotkey>; 3],
    pub accumulate: bool,
    pub melt: bool,
    pub snow_on_windows: bool,
    pub behind_windows: bool,
    pub show_in_screenshots: bool,
    pub welcomed: bool,
}

impl Settings {
    pub fn load() -> Settings {
        let calm = !animations_enabled();
        let snowfall = read(w!("snowfall")).and_then(|i| Snowfall::ALL.get(i as usize).copied());
        let mut palette = [[1.0; 3]; MAX_COLORS];
        let mut palette_len = 0;
        for hex in read_text(w!("palette")).unwrap_or_default().split('|').take(MAX_COLORS) {
            let channel = |at: usize| hex.get(at..at + 2).and_then(|c| u8::from_str_radix(c, 16).ok()).map(|v| v as f32 / 255.0);
            if let (Some(r), Some(g), Some(b)) = (channel(0), channel(2), channel(4)) {
                palette[palette_len] = [r, g, b];
                palette_len += 1;
            }
        }
        let index = |name: PCWSTR, default: usize, count: usize| read(name).map_or(default, |i| i as usize).min(count - 1);
        // Off used to be one of the amounts; it is the switch now.
        let paused = read(w!("paused")).is_some_and(|v| v != 0) || snowfall == Some(Snowfall::Off);
        let snowfall = snowfall.filter(|&s| s != Snowfall::Off);
        Settings {
            paused,
            snowfall: snowfall.unwrap_or(if calm { Snowfall::Light } else { Snowfall::Steady }),
            weather: read(w!("weather")).and_then(|i| Weather::ALL.get(i as usize).copied()).unwrap_or(Weather::Snow),
            size: index(w!("flakeSize"), 1, SIZES.len()),
            wind: index(w!("wind"), 1, WINDS.len()),
            opacity: index(w!("opacity"), 0, OPACITIES.len()),
            // 30 fps unless chosen otherwise: snow drifting doesn't need more, and it halves the work.
            frame_cap: index(w!("frameCap"), 2, FRAME_CAPS.len()),
            fade_while_typing: read(w!("fadeWhileTyping")).is_some_and(|v| v != 0),
            local_weather: read(w!("localWeather")).is_some_and(|v| v != 0),
            seasonal: read(w!("seasonal")).is_some_and(|v| v != 0),
            mouse_wind: index(w!("mouseWind"), 1, MOUSE_WINDS.len()),
            glitter: index(w!("glitter"), 1, GLITTERS.len()),
            custom_colors: read(w!("customColors")).is_some_and(|v| v != 0),
            palette,
            palette_len,
            schedule: index(w!("schedule"), 0, SCHEDULES.len()),
            hotkeys: SHORTCUTS.map(|(_, name, default)| match read(name) {
                None => Some(default),
                Some(0) => None,
                Some(packed) => Some(Hotkey { modifiers: packed >> 16, key: packed & 0xffff }),
            }),
            accumulate: read(w!("accumulate")).is_none_or(|v| v != 0),
            melt: read(w!("meltSnow")).is_none_or(|v| v != 0),
            snow_on_windows: read(w!("snowOnWindows")).is_none_or(|v| v != 0),
            behind_windows: read(w!("behindWindows")).is_some_and(|v| v != 0),
            show_in_screenshots: read(w!("showInScreenshots")).is_some_and(|v| v != 0),
            welcomed: read(w!("welcomed")).is_some_and(|v| v != 0),
        }
    }

    pub fn save(&self) {
        let snowfall = Snowfall::ALL.iter().position(|&s| s == self.snowfall).unwrap_or(2);
        write(w!("snowfall"), snowfall as u32);
        write(w!("paused"), self.paused as u32);
        write(w!("weather"), Weather::ALL.iter().position(|&w| w == self.weather).unwrap_or(0) as u32);
        write(w!("flakeSize"), self.size as u32);
        write(w!("wind"), self.wind as u32);
        write(w!("opacity"), self.opacity as u32);
        write(w!("frameCap"), self.frame_cap as u32);
        write(w!("fadeWhileTyping"), self.fade_while_typing as u32);
        write(w!("localWeather"), self.local_weather as u32);
        write(w!("seasonal"), self.seasonal as u32);
        write(w!("mouseWind"), self.mouse_wind as u32);
        write(w!("glitter"), self.glitter as u32);
        write(w!("customColors"), self.custom_colors as u32);
        let palette: Vec<String> = self.palette[..self.palette_len.min(MAX_COLORS)]
            .iter()
            .map(|c| c.iter().map(|v| format!("{:02x}", (v * 255.0).round() as u8)).collect())
            .collect();
        write_text(w!("palette"), &palette.join("|"));
        write(w!("schedule"), self.schedule as u32);
        for ((_, name, _), hotkey) in SHORTCUTS.iter().zip(self.hotkeys) {
            write(*name, hotkey.map_or(0, |hotkey| hotkey.modifiers << 16 | hotkey.key));
        }
        write(w!("accumulate"), self.accumulate as u32);
        write(w!("meltSnow"), self.melt as u32);
        write(w!("snowOnWindows"), self.snow_on_windows as u32);
        write(w!("behindWindows"), self.behind_windows as u32);
        write(w!("showInScreenshots"), self.show_in_screenshots as u32);
        write(w!("welcomed"), self.welcomed as u32);
    }

    pub fn look(&self) -> Look {
        Look {
            weather: self.weather,
            size: SIZES[self.size].1,
            wind: WINDS[self.wind].1,
            mouse_wind: MOUSE_WINDS[self.mouse_wind].1,
            palette: self.palette,
            colors: if self.custom_colors { self.palette_len.min(MAX_COLORS) } else { 0 },
        }
    }

    pub fn glitter(&self) -> f32 {
        GLITTERS[self.glitter].1
    }

    /// Whether the schedule lets the snow show at this hour.
    pub fn scheduled(&self, hour: u32) -> bool {
        let (_, from, to) = SCHEDULES[self.schedule];
        if from == to {
            true
        } else if from < to {
            (from..to).contains(&hour)
        } else {
            hour >= from || hour < to
        }
    }

    pub fn opacity(&self) -> f32 {
        OPACITIES[self.opacity].1
    }

    pub fn frame_cap(&self) -> u32 {
        FRAME_CAPS[self.frame_cap].1
    }
}

/// Programs whose windows get no snow, by executable name in lowercase, such as `code.exe`.
pub fn excluded_apps() -> Vec<String> {
    read_text(w!("excludedApps"))
        .map(|text| text.split('|').filter(|name| !name.is_empty()).map(str::to_lowercase).collect())
        .unwrap_or_default()
}

pub fn set_excluded_apps(apps: &[String]) {
    write_text(w!("excludedApps"), &apps.join("|"));
}

fn write_text(name: PCWSTR, text: &str) {
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_SZ.0, Some(text.as_ptr().cast()), (text.len() * 2) as u32);
    }
}

fn read_text(name: PCWSTR) -> Option<String> {
    let mut buffer = [0u16; 4096];
    let mut size = (buffer.len() * 2) as u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast::<c_void>()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buffer[..length]))
}

pub fn opens_at_login() -> bool {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast::<c_void>()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return false;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    command().is_some_and(|command| command[..command.len() - 1] == buffer[..length])
}

/// Turns Open at Login on the first time this version runs, once: switched off later in the menu, it stays off.
pub fn open_at_login_by_default() {
    if read(w!("loginDefaulted")).is_some() {
        return;
    }
    let status = set_opens_at_login(true);
    if status == ERROR_SUCCESS {
        write(w!("loginDefaulted"), 1);
    }
}

pub fn set_opens_at_login(on: bool) -> WIN32_ERROR {
    unsafe {
        if !on {
            return RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE);
        }
        let Some(command) = command() else { return WIN32_ERROR(1) };
        RegSetKeyValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE, REG_SZ.0, Some(command.as_ptr().cast()), (command.len() * 2) as u32)
    }
}

/// The quoted path of this executable, NUL-terminated.
fn command() -> Option<Vec<u16>> {
    let path = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", path.display()).encode_utf16().chain([0]).collect())
}

fn read(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_DWORD, None, Some((&raw mut value).cast::<c_void>()), Some(&mut size)) };
    (status == ERROR_SUCCESS).then_some(value)
}

fn write(name: PCWSTR, value: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_DWORD.0, Some((&raw const value).cast::<c_void>()), 4);
    }
}

/// The "Animation effects" setting: off means the person prefers less motion.
pub fn animations_enabled() -> bool {
    let mut enabled = BOOL(1);
    let result = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some((&raw mut enabled).cast::<c_void>()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))
    };
    result.is_err() || enabled.as_bool()
}
