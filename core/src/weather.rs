//! Local weather from Open-Meteo, and the seasons where someone lives, turned into what falls on the desktop.

use crate::simulation::{Snowfall, Weather};

/// What the sky is doing now, from a forecast's `current` block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    /// Rain or snow and how much; `None` under a dry sky, when the season decides instead.
    pub falling: Option<(Snowfall, Weather)>,
    /// Scales the breeze, from the wind speed outside.
    pub wind: f32,
}

/// The weather of a season at a latitude: snow in winter, blossoms in spring, stars on summer nights, leaves in autumn.
/// South of the equator the seasons are the other way round; near it, where there are none to speak of, blossoms.
pub fn season(month: u32, latitude: f64) -> Weather {
    if latitude.abs() < 23.5 {
        return Weather::Blossoms;
    }
    let month = if latitude < 0.0 { (month + 5) % 12 + 1 } else { month };
    match month {
        3..=5 => Weather::Blossoms,
        6..=8 => Weather::Stars,
        9..=11 => Weather::Leaves,
        _ => Weather::Snow,
    }
}

/// The path of the forecast request for a place, rounded to about a kilometre so the request doesn't pinpoint anyone.
pub fn forecast_path(latitude: f64, longitude: f64) -> String {
    format!("/v1/forecast?latitude={latitude:.2}&longitude={longitude:.2}&current=weather_code,wind_speed_10m&wind_speed_unit=kmh")
}

/// Reads Open-Meteo's answer.
pub fn conditions(json: &str) -> Option<Conditions> {
    // `current_units` comes first and names the same keys, so read only past `"current":`.
    let current = &json[json.find("\"current\":")? + 10..];
    let code = number(current, "weather_code")? as u32;
    let wind_speed = number(current, "wind_speed_10m").unwrap_or(12.0) as f32;
    let falling = match code {
        51 | 53 | 55 | 56 | 57 | 61 | 66 | 80 => Some((Snowfall::Light, Weather::Rain)),
        63 | 81 => Some((Snowfall::Steady, Weather::Rain)),
        65 | 67 | 82 | 95 | 96 | 99 => Some((Snowfall::Heavy, Weather::Rain)),
        71 | 77 | 85 => Some((Snowfall::Light, Weather::Snow)),
        73 => Some((Snowfall::Steady, Weather::Snow)),
        75 | 86 => Some((Snowfall::Heavy, Weather::Snow)),
        _ => None,
    };
    Some(Conditions { falling, wind: (wind_speed / 12.0).clamp(0.3, 3.0) })
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
        assert_eq!(conditions(ANSWER), Some(Conditions { falling: Some((Snowfall::Steady, Weather::Snow)), wind: 24.5 / 12.0 }));
        let clear = ANSWER.replace("\"weather_code\":73", "\"weather_code\":0");
        assert_eq!(conditions(&clear).map(|c| c.falling), Some(None));
        let storm = ANSWER.replace("\"weather_code\":73", "\"weather_code\":95");
        assert_eq!(conditions(&storm).and_then(|c| c.falling), Some((Snowfall::Heavy, Weather::Rain)));
        assert_eq!(conditions("{}"), None);
    }

    #[test]
    fn seasons_follow_the_hemisphere() {
        // Berlin and Sydney in November, and in July.
        assert_eq!(season(11, 52.5), Weather::Leaves);
        assert_eq!(season(11, -33.9), Weather::Blossoms);
        assert_eq!(season(7, 52.5), Weather::Stars);
        assert_eq!(season(7, -33.9), Weather::Snow);
        assert_eq!(season(1, 52.5), Weather::Snow);
        assert_eq!(season(1, -33.9), Weather::Stars);
        // Singapore has no seasons to speak of.
        assert_eq!(season(1, 1.3), Weather::Blossoms);
    }

    #[test]
    fn rounds_the_place() {
        assert_eq!(
            forecast_path(52.520_08, 13.404_95),
            "/v1/forecast?latitude=52.52&longitude=13.40&current=weather_code,wind_speed_10m&wind_speed_unit=kmh"
        );
    }
}
