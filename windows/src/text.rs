//! The menu and messages in German when Windows is set to German, in English otherwise.

use std::sync::OnceLock;

use windows::Win32::Globalization::GetUserDefaultUILanguage;

fn german() -> bool {
    static GERMAN: OnceLock<bool> = OnceLock::new();
    // The primary language is the low ten bits; 0x07 is German.
    *GERMAN.get_or_init(|| unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x07)
}

/// `text` in the person's language. Placeholders `{}` are filled in order by `fill`.
pub fn t(text: &str) -> String {
    if !german() {
        return text.to_string();
    }
    let translated = match text {
        "Snowfall" => "Schneefall",
        "Off" => "Aus",
        "Light" => "Leicht",
        "Steady" => "Mittel",
        "Heavy" => "Stark",
        "Variable" => "Wechselnd",
        "Weather" => "Wetter",
        "Snow" => "Schnee",
        "Rain" => "Regen",
        "Blossoms" => "Blüten",
        "Leaves" => "Laub",
        "Seasonal" => "Nach Jahreszeit",
        "Match Local Weather" => "Echtes Wetter vor Ort",
        "Look" => "Aussehen",
        "Flake Size" => "Flockengröße",
        "Small" => "Klein",
        "Normal" => "Normal",
        "Large" => "Groß",
        "Wind" => "Wind",
        "Still" => "Windstill",
        "Breezy" => "Brise",
        "Stormy" => "Stürmisch",
        "Mouse Wind" => "Wind von der Maus",
        "Gentle" => "Sanft",
        "Strong" => "Stark",
        "Color" => "Farbe",
        "Natural" => "Natürlich",
        "Ice Blue" => "Eisblau",
        "Gold" => "Gold",
        "Pink" => "Rosa",
        "Mint" => "Mint",
        "Lavender" => "Lavendel",
        "Glitter" => "Glitzern",
        "Sparkly" => "Viel",
        "Opacity" => "Deckkraft",
        "Frame Rate" => "Bildrate",
        "Automatic" => "Automatisch",
        "Eco (20 fps)" => "Sparsam (20 fps)",
        "Fade While Typing" => "Beim Tippen ausblenden",
        "Schedule" => "Zeitplan",
        "Always" => "Immer",
        "Evenings (17–23)" => "Abends (17–23 Uhr)",
        "Daytime (8–17)" => "Tagsüber (8–17 Uhr)",
        "Nights (20–6)" => "Nachts (20–6 Uhr)",
        "Accumulate" => "Liegen bleiben",
        "Melt Snow" => "Schnee schmelzen",
        "Snow on Windows" => "Schnee auf Fenstern",
        "Clear Snow" => "Schnee wegfegen",
        "Throw Snowball" => "Schneeball werfen",
        "Build Snowman" => "Schneemann bauen",
        "Fun" => "Spaß",
        "Keep Snow Off {}" => "Kein Schnee auf {}",
        "Switch to a program, then come back here" => "Erst zu einem Programm wechseln, dann hierher",
        "Snow-Free Programs" => "Schneefreie Programme",
        "Behind Windows" => "Hinter Fenstern",
        "Show in Screenshots" => "In Screenshots zeigen",
        "Open at Login" => "Beim Anmelden starten",
        "Keyboard Shortcuts" => "Tastenkürzel",
        "Enable Shortcuts" => "Tastenkürzel aktiv",
        "Ctrl+Alt+S  Snow on/off" => "Strg+Alt+S  Schnee an/aus",
        "Ctrl+Alt+C  Clear snow" => "Strg+Alt+C  Schnee wegfegen",
        "Ctrl+Alt+W  Next weather" => "Strg+Alt+W  Nächstes Wetter",
        "Ctrl+Alt+B  Snowball at the pointer" => "Strg+Alt+B  Schneeball zum Mauszeiger",
        "Ctrl+Alt+M  Snowman at the pointer" => "Strg+Alt+M  Schneemann beim Mauszeiger",
        "Double-click the tray icon: snow on/off" => "Doppelklick aufs Symbol: Schnee an/aus",
        "Check for Updates…" => "Nach Updates suchen…",
        "Quit" => "Beenden",
        "Not enough snow here" => "Hier liegt zu wenig Schnee",
        "Let more snow settle, or try where the drift is deepest." => "Erst mehr Schnee liegen lassen oder dort probieren, wo am meisten liegt.",
        "Couldn't get the local weather.\n\n{}" => "Das Wetter vor Ort ließ sich nicht abrufen.\n\n{}",
        "DesktopFlora {} is available" => "DesktopFlora {} ist verfügbar",
        "Click to install it." => "Zum Installieren klicken.",
        "You're up to date.\n\nDesktopFlora {} is the latest version." => "Alles aktuell.\n\nDesktopFlora {} ist die neueste Version.",
        "Couldn't check for updates.\n\n{}" => "Die Suche nach Updates hat nicht geklappt.\n\n{}",
        "DesktopFlora {} is available. You have {}.\n\nOpen the download page?" => {
            "DesktopFlora {} ist verfügbar. Installiert ist {}.\n\nDownload-Seite öffnen?"
        }
        "DesktopFlora {} is available. You have {}.\n\nInstall it now? DesktopFlora downloads it, checks it against the release's checksum and restarts." => {
            "DesktopFlora {} ist verfügbar. Installiert ist {}.\n\nJetzt installieren? DesktopFlora lädt sie herunter, prüft die Prüfsumme und startet neu."
        }
        "Couldn't install the update.\n\n{}\n\nOpen the download page instead?" => {
            "Das Update ließ sich nicht installieren.\n\n{}\n\nStattdessen die Download-Seite öffnen?"
        }
        _ => text,
    };
    translated.to_string()
}

/// Fills the `{}` placeholders of a translated text in order.
pub fn fill(text: &str, values: &[&str]) -> String {
    let mut result = String::new();
    let mut parts = text.split("{}");
    if let Some(first) = parts.next() {
        result.push_str(first);
    }
    for (i, part) in parts.enumerate() {
        result.push_str(values.get(i).copied().unwrap_or(""));
        result.push_str(part);
    }
    result
}
