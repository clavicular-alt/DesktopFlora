//! Local weather from Open-Meteo, turned into what falls on the desktop.

use crate::simulation::{Snowfall, Weather};

/// What the sky is doing now, from a forecast's `current` block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    pub snowfall: Snowfall,
    pub weather: Weather,
    /// Scales the breeze, from the wind speed outside.
    pub wind: f32,
}

/// The path of the forecast request for a place, rounded to about a kilometre so the request doesn't pinpoint anyone.
pub fn forecast_path(latitude: f64, longitude: f64) -> String {
    format!("/v1/forecast?latitude={latitude:.2}&longitude={longitude:.2}&current=weather_code,wind_speed_10m&wind_speed_unit=kmh")
}

/// Reads Open-Meteo's answer. Clear skies, clouds and fog mean nothing falls.
pub fn conditions(json: &str) -> Option<Conditions> {
    // `current_units` comes first and names the same keys, so read only past `"current":`.
    let current = &json[json.find("\"current\":")? + 10..];
    let code = number(current, "weather_code")? as u32;
    let wind_speed = number(current, "wind_speed_10m").unwrap_or(12.0) as f32;
    let (snowfall, weather) = match code {
        51 | 53 | 55 | 56 | 57 | 61 | 66 | 80 => (Snowfall::Light, Weather::Rain),
        63 | 81 => (Snowfall::Steady, Weather::Rain),
        65 | 67 | 82 | 95 | 96 | 99 => (Snowfall::Heavy, Weather::Rain),
        71 | 77 | 85 => (Snowfall::Light, Weather::Snow),
        73 => (Snowfall::Steady, Weather::Snow),
        75 | 86 => (Snowfall::Heavy, Weather::Snow),
        _ => (Snowfall::Off, Weather::Snow),
    };
    Some(Conditions { snowfall, weather, wind: (wind_speed / 12.0).clamp(0.3, 3.0) })
}

fn number(json: &str, key: &str) -> Option<f64> {
    let rest = &json[json.find(&format!("\"{key}\""))? + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"latitude":52.52,"current_units":{"time":"iso8601","weather_code":"wmo code","wind_speed_10m":"km/h"},
        "current":{"time":"2026-01-10T12:00","interval":900,"weather_code":73,"wind_speed_10m":24.5}}"#;

    #[test]
    fn reads_current_conditions() {
        assert_eq!(conditions(ANSWER), Some(Conditions { snowfall: Snowfall::Steady, weather: Weather::Snow, wind: 24.5 / 12.0 }));
        let clear = ANSWER.replace("\"weather_code\":73", "\"weather_code\":0");
        assert_eq!(conditions(&clear).map(|c| c.snowfall), Some(Snowfall::Off));
        let storm = ANSWER.replace("\"weather_code\":73", "\"weather_code\":95");
        assert_eq!(conditions(&storm).map(|c| (c.snowfall, c.weather)), Some((Snowfall::Heavy, Weather::Rain)));
        assert_eq!(conditions("{}"), None);
    }

    #[test]
    fn rounds_the_place() {
        assert_eq!(
            forecast_path(52.520_08, 13.404_95),
            "/v1/forecast?latitude=52.52&longitude=13.40&current=weather_code,wind_speed_10m&wind_speed_unit=kmh"
        );
    }
}
