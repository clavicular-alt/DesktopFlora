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
        "Choose" => "Selbst wählen",
        "Season" => "Jahreszeit",
        "Outside" => "Wetter vor Ort",
        "Size" => "Größe",
        "My Colors" => "Eigene Farben",
        "Pick colors below" => "Unten Farben auswählen",
        "Click one to remove it" => "Antippen zum Entfernen",
        "Full: remove one to add another" => "Voll: eine entfernen für eine neue",
        "Winter" => "Winter",
        "Spring" => "Frühling",
        "Summer" => "Sommer",
        "Autumn" => "Herbst",
        "Dry outside: {} for the {}" => "Draußen trocken: {} zum {}",
        "Place: your location from Windows, rounded to about 1 km" => "Ort: dein Windows-Standort, auf etwa 1 km gerundet",
        "your region" => "deiner Region",
        "Season in {}, from the region set in Windows" => "Jahreszeit in {}, aus der Region in Windows",
        "{} · {}" => "{} · {}",
        "Paused" => "Pausiert",
        "Behavior" => "Verhalten",
        "More" => "Mehr",
        "What falls" => "Was fällt",
        "Amount" => "Menge",
        "Stars" => "Sterne",
        "+ Keep Snow Off {}" => "+ Kein Schnee auf {}",
        "Switch to a program, then open this again" => "Erst zu einem Programm wechseln, dann hier öffnen",
        "and {} more" => "und {} weitere",
        "Snow on/off" => "An / Aus",
        "Clear snow" => "Wegfegen",
        "Next weather" => "Nächstes Wetter",
        "Press keys…" => "Tasten drücken…",
        "{} (taken)" => "{} (belegt)",
        "None" => "Keins",
        "Click a shortcut, then press the new keys. Delete removes it." => "Kürzel anklicken, dann neue Tasten drücken. Entf löscht es.",
        "Ctrl" => "Strg",
        "Shift" => "Umschalt",
        "Outside right now: {}" => "Draußen gerade: {}",
        "Asking for the weather outside…" => "Wetter wird abgefragt…",
        "Outside the schedule right now" => "Gerade außerhalb des Zeitplans",
        "Windows didn't share the location. Turn on Location and \"Let desktop apps access your location\" in Settings → Privacy & security → Location." => {
            "Windows hat den Standort nicht freigegeben. In Einstellungen → Datenschutz und Sicherheit → Position die Ortung und „Desktop-Apps den Zugriff auf Ihren Standort erlauben“ einschalten."
        }
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
        "Look" => "Aussehen",
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
        "Glitter" => "Glitzern",
        "Color" => "Farbe",
        "Natural" => "Natürlich",
        "Sparkly" => "Viel",
        "Opacity" => "Deckkraft",
        "Frame Rate" => "Bildrate",
        "Eco" => "Sparsam",
        "Fade While Typing" => "Beim Tippen ausblenden",
        "Schedule" => "Zeitplan",
        "Always" => "Immer",
        "Evenings" => "Abends",
        "Daytime" => "Tagsüber",
        "Nights" => "Nachts",
        "Accumulate" => "Liegen bleiben",
        "Melt Snow" => "Schnee schmelzen",
        "Snow on Windows" => "Schnee auf Fenstern",
        "Clear Snow" => "Schnee wegfegen",
        "Snow-Free Programs" => "Schneefreie Programme",
        "Behind Windows" => "Hinter Fenstern",
        "Show in Screenshots" => "In Screenshots zeigen",
        "Open at Login" => "Mit Windows starten",
        "Keyboard Shortcuts" => "Tastenkürzel",
        "Check for Updates…" => "Nach Updates suchen…",
        "Quit" => "Beenden",
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
