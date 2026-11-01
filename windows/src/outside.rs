//! The weather where the computer is: Windows locates it, if allowed, and Open-Meteo says what the sky is doing there.
//! Without a location, the seasons follow the country Windows is set to.

use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};
use windows::Win32::Globalization::{GEO_FRIENDLYNAME, GEO_LATITUDE, GEOCLASS_NATION, GetGeoInfoW, GetUserGeoID};

use snowhush_core::weather::{Conditions, conditions, forecast_path};

use crate::update::fetch;

/// What is falling outside now, and the latitude it was asked for. Errors explain what to do, as turning on location access.
pub fn now() -> Result<(Conditions, f64), String> {
    let (latitude, longitude) = locate()?;
    let body = fetch("api.open-meteo.com", &forecast_path(latitude, longitude), "application/json")?;
    let conditions =
        conditions(&String::from_utf8_lossy(&body)).ok_or_else(|| "the weather service gave an answer DesktopFlora doesn't understand".to_string())?;
    Ok((conditions, latitude))
}

/// The country set in Windows' region settings, by name in the person's language, and its latitude, for the seasons;
/// central Europe if it can't be read.
pub fn home() -> (String, f64) {
    unsafe {
        let nation = GetUserGeoID(GEOCLASS_NATION);
        let info = |kind| {
            let mut buffer = [0u16; 128];
            let length = GetGeoInfoW(nation, kind, Some(&mut buffer), 0);
            (length > 1).then(|| String::from_utf16_lossy(&buffer[..length as usize - 1]).trim().to_string())
        };
        let latitude = info(GEO_LATITUDE).and_then(|text| text.replace(',', ".").parse().ok()).unwrap_or(50.0);
        (info(GEO_FRIENDLYNAME).unwrap_or_default(), latitude)
    }
}

fn locate() -> Result<(f64, f64), String> {
    let denied = || {
        crate::text::t(
            "Windows didn't share the location. Turn on Location and \"Let desktop apps access your location\" in Settings → Privacy & security → Location.",
        )
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
