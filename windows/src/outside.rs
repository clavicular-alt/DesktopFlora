//! The weather where the computer is: Windows locates it, if allowed, and Open-Meteo says what the sky is doing there.

use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};

use snowhush_core::weather::{Conditions, conditions, forecast_path};

use crate::update::fetch;

/// What is falling outside now. Errors explain what to do, as turning on location access.
pub fn now() -> Result<Conditions, String> {
    let (latitude, longitude) = locate()?;
    let body = fetch("api.open-meteo.com", &forecast_path(latitude, longitude), "application/json")?;
    conditions(&String::from_utf8_lossy(&body)).ok_or_else(|| "the weather service gave an answer DesktopFlora doesn't understand".to_string())
}

fn locate() -> Result<(f64, f64), String> {
    let denied = || {
        "Windows didn't share the location. Turn on Location and \"Let desktop apps access your location\" in Settings → Privacy & security → Location."
            .to_string()
    };
    let access = Geolocator::RequestAccessAsync().and_then(|operation| operation.join()).map_err(|error| error.message())?;
    if access != GeolocationAccessStatus::Allowed {
        return Err(denied());
    }
    let locator = Geolocator::new().map_err(|error| error.message())?;
    // The town is enough for the weather; no need to wake GPS.
    let _ = locator.SetDesiredAccuracy(PositionAccuracy::Default);
    let position = locator
        .GetGeopositionAsync()
        .and_then(|operation| operation.join())
        .and_then(|position| position.Coordinate())
        .and_then(|coordinate| coordinate.Point())
        .and_then(|point| point.Position())
        .map_err(|_| denied())?;
    Ok((position.Latitude, position.Longitude))
}
