use std::{sync::LazyLock, time::Duration};

use chrono::{NaiveDate, NaiveDateTime};
use iced::{
    Border, Degrees, Element, Length, Padding, Rotation, Theme,
    alignment::{Horizontal, Vertical},
    core::svg::Handle,
    widget::{Column, MouseArea, Row, Svg, column, container, row, svg, text},
};
use itertools::izip;
use serde::{Deserialize, Deserializer};

use crate::{
    components::scrollable,
    config::WeatherLocation,
    i18n::{UnitSystem, chrono_locale, unit_system},
    t,
    theme::{Paint, use_theme},
};

use super::{Message, Tempo};

impl Tempo {
    pub(super) fn weather<'a>(&'a self) -> Option<Element<'a, Message>> {
        let (space, font_size, radius) = use_theme(|t| (t.space, t.font_size, t.radius));
        let locale = chrono_locale();
        let units = unit_system();
        let temp = units.temperature_symbol();
        let wind = self.config.resolved_wind_speed_unit().symbol();
        let location_visible = self.location_visible;
        self.weather_data
            .as_ref()
            .zip(self.location.as_ref())
            .map(|(data, location)| {
                let inner_element: Element<'a, Message> = if location_visible {
                    let display_time =
                        self.time_str("%R", self.current_timezone_index, Some(data.current.time));
                    text(format!(
                        "{}{} - {}",
                        location.city,
                        if location.region_name.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", location.region_name)
                        },
                        display_time
                    ))
                    .size(font_size.sm)
                    .into()
                } else {
                    container(text("•••••").size(font_size.sm))
                        .style(move |theme: &Theme| container::Style {
                            background: Some(
                                Paint::surface(
                                    theme,
                                    theme.extended_palette().background.strong.color,
                                )
                                .into(),
                            ),
                            border: Border::default().rounded(radius.sm),
                            ..Default::default()
                        })
                        .into()
                };

                let location_element: Element<'a, Message> = MouseArea::new(inner_element)
                    .on_press(Message::ToggleLocationVisibility)
                    .into();

                column!(
                    container(
                        row!(
                            weather_icon(data.current.weather_code, data.current.is_day > 0)
                                .height(font_size.xxl)
                                .width(Length::Shrink),
                            column!(
                                location_element,
                                text(weather_description(data.current.weather_code)),
                                row!(
                                    text(format!("{}{temp}", data.current.temperature_2m)),
                                    text(t!(
                                        "tempo-feels-like",
                                        value = data.current.apparent_temperature.round(),
                                        unit = temp,
                                    ))
                                    .size(font_size.sm)
                                )
                                .align_y(Vertical::Bottom)
                                .spacing(space.sm),
                            )
                            .width(Length::FillPortion(2))
                            .spacing(space.xs),
                            column!(
                                row!(
                                    svg(Handle::from_memory(include_bytes!(
                                        "../../../assets/weather_icon/drop.svg"
                                    )))
                                    .width(Length::Shrink)
                                    .height(font_size.lg),
                                    column!(
                                        text(t!("tempo-humidity"))
                                            .size(font_size.xs)
                                            .align_x(Horizontal::Right)
                                            .width(Length::Fill),
                                        text(format!("{}%", data.current.relative_humidity_2m))
                                            .align_x(Horizontal::Right)
                                            .size(font_size.xs)
                                            .width(Length::Fill),
                                    )
                                    .spacing(space.xxs)
                                )
                                .align_y(Vertical::Center)
                                .spacing(space.sm),
                                row!(
                                    svg(Handle::from_memory(include_bytes!(
                                        "../../../assets/weather_icon/wind.svg"
                                    )))
                                    .height(font_size.lg)
                                    .width(Length::Shrink)
                                    .rotation(
                                        Rotation::Floating(
                                            Degrees(data.current.wind_direction_10m as f32 + 90.)
                                                .into()
                                        )
                                    ),
                                    column!(
                                        text(t!("tempo-wind"))
                                            .size(font_size.xs)
                                            .align_x(Horizontal::Right)
                                            .width(Length::Fill),
                                        text(format!(
                                            "{} {wind}",
                                            data.current.wind_speed_10m.round()
                                        ))
                                        .align_x(Horizontal::Right)
                                        .size(font_size.xs)
                                        .width(Length::Fill),
                                    )
                                    .spacing(space.xxs)
                                )
                                .align_y(Vertical::Center)
                                .spacing(space.sm),
                            )
                            .width(Length::Fill)
                            .spacing(space.xs),
                        )
                        .spacing(space.lg)
                        .align_y(Vertical::Center)
                        .width(Length::Fill),
                    )
                    .padding(space.md)
                    .style(crate::theme::card_style(radius.lg)),
                    container(
                        scrollable(
                            Row::with_children({
                                let mut time = data
                                    .hourly
                                    .time
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, t)| **t > data.current.time)
                                    .take(23)
                                    .peekable();
                                let start_index = time.peek().map(|(index, _)| *index).unwrap_or(0);

                                izip!(
                                    time.map(|(_, v)| v),
                                    data.hourly.weather_code.iter().enumerate().filter_map(
                                        |(i, v)| if i >= start_index { Some(v) } else { None }
                                    ),
                                    data.hourly.temperature_2m.iter().enumerate().filter_map(
                                        |(i, v)| if i >= start_index { Some(v) } else { None }
                                    ),
                                    data.hourly.is_day.iter().enumerate().filter_map(|(i, v)| {
                                        if i >= start_index { Some(v) } else { None }
                                    }),
                                )
                                .map(|(hour_time, weather_code, temp_value, is_day)| {
                                    let display_time = self.time_str(
                                        "%H:%M",
                                        self.current_timezone_index,
                                        Some(*hour_time),
                                    );
                                    column!(
                                        text(format!("{}{temp}", temp_value.round())),
                                        weather_icon(*weather_code, *is_day > 0)
                                            .height(font_size.md)
                                            .width(Length::Shrink),
                                        text(display_time).size(font_size.sm)
                                    )
                                    .spacing(space.xs)
                                    .align_x(Horizontal::Center)
                                    .into()
                                })
                                .collect::<Vec<_>>()
                            })
                            .spacing(space.sm)
                            .padding(Padding::default().bottom(space.sm))
                        )
                        .horizontal()
                    )
                    .padding(space.sm)
                    .style(crate::theme::card_style(radius.lg)),
                    Column::with_children(
                        izip!(
                            &data.daily.time,
                            &data.daily.weather_code,
                            &data.daily.temperature_2m_min,
                            &data.daily.temperature_2m_max,
                            &data.daily.wind_direction_10m_dominant,
                            &data.daily.wind_speed_10m_max,
                            &data.daily.relative_humidity_2m_mean,
                        )
                        .skip(1)
                        .enumerate()
                        .map(
                            |(
                                index,
                                (
                                    time,
                                    weather_code,
                                    temp_min,
                                    temp_max,
                                    wind_dir,
                                    wind_speed,
                                    humidity,
                                ),
                            )| {
                                let last_index = data.daily.time.len() - 2;
                                container(
                                    row!(
                                        text(
                                            time.format_localized("%a, %d %b", locale).to_string()
                                        )
                                        .width(Length::FillPortion(5)),
                                        row!(
                                            weather_icon(*weather_code, true)
                                                .height(font_size.md)
                                                .width(Length::Fixed(font_size.md)),
                                            text(format!(
                                                "{}{temp}/{}{temp}",
                                                temp_max.round(),
                                                temp_min.round()
                                            ))
                                        )
                                        .spacing(space.xxs)
                                        .align_y(Vertical::Center)
                                        .width(Length::FillPortion(5)),
                                        column!(
                                            row!(
                                                svg(Handle::from_memory(include_bytes!(
                                                    "../../../assets/weather_icon/drop.svg"
                                                )))
                                                .height(font_size.sm)
                                                .width(Length::Fixed(font_size.sm)),
                                                text(format!("{humidity}%")).size(font_size.sm)
                                            )
                                            .spacing(space.xxs)
                                            .align_y(Vertical::Center),
                                            row!(
                                                svg(Handle::from_memory(include_bytes!(
                                                    "../../../assets/weather_icon/wind.svg"
                                                )))
                                                .height(font_size.sm)
                                                .width(Length::Fixed(font_size.sm))
                                                .rotation(Rotation::Floating(
                                                    Degrees(*wind_dir as f32 + 90.).into()
                                                )),
                                                text(format!("{} {wind}", wind_speed.round()))
                                                    .size(font_size.sm)
                                            )
                                            .spacing(space.xxs)
                                            .align_y(Vertical::Center)
                                        )
                                        .width(Length::FillPortion(3))
                                    )
                                    .spacing(space.sm)
                                    .align_y(Vertical::Center),
                                )
                                .padding(space.sm)
                                .style(crate::theme::card_style(iced::border::Radius {
                                    top_left: if index == 0 { radius.lg } else { radius.sm },
                                    top_right: if index == 0 { radius.lg } else { radius.sm },
                                    bottom_right: if index == last_index {
                                        radius.lg
                                    } else {
                                        radius.sm
                                    },
                                    bottom_left: if index == last_index {
                                        radius.lg
                                    } else {
                                        radius.sm
                                    },
                                }))
                                .into()
                            }
                        )
                    )
                    .spacing(space.xxs)
                )
                .spacing(space.sm)
                .into()
            })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct GeoLocations {
    results: Vec<GeoLocation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoLocation {
    latitude: f32,
    longitude: f32,
    name: String,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    country: Option<String>,
}

impl From<GeoLocation> for Location {
    fn from(value: GeoLocation) -> Self {
        let region_name = if let Some(admin1) = &value.admin1 {
            if let Some(country) = &value.country {
                if admin1 == country || admin1 == &value.name {
                    country.clone()
                } else {
                    admin1.clone()
                }
            } else {
                admin1.clone()
            }
        } else {
            value.country.unwrap_or_default()
        };

        Location {
            latitude: value.latitude,
            longitude: value.longitude,
            city: value.name,
            region_name,
        }
    }
}

/// Response of `https://ipwho.is/`, a free HTTPS IP geolocation service.
/// On lookup failure the service still answers with HTTP 200 and
/// `success: false` plus a `message`, so both have to be checked.
#[derive(Clone, Debug, Deserialize)]
struct IpLocation {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    latitude: Option<f32>,
    #[serde(default)]
    longitude: Option<f32>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    region: Option<String>,
}

impl IpLocation {
    fn into_location(self) -> anyhow::Result<Location> {
        if !self.success {
            anyhow::bail!(
                "IP geolocation lookup failed: {}",
                self.message.as_deref().unwrap_or("unknown error")
            );
        }

        Ok(Location {
            latitude: self
                .latitude
                .ok_or_else(|| anyhow::anyhow!("IP geolocation returned no latitude"))?,
            longitude: self
                .longitude
                .ok_or_else(|| anyhow::anyhow!("IP geolocation returned no longitude"))?,
            city: self.city.unwrap_or_default(),
            region_name: self.region.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Location {
    pub latitude: f32,
    pub longitude: f32,
    pub city: String,
    pub region_name: String,
}

/// Upper bound for a response body. The largest real payload, the 7-day
/// forecast, is about 6 KiB.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

static HTTP_CLIENT: LazyLock<Result<reqwest::Client, reqwest::Error>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!(
            "ashell/",
            env!("CARGO_PKG_VERSION"),
            " (+",
            env!("CARGO_PKG_REPOSITORY"),
            ")"
        ))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
});

/// Sends a GET request and returns the response body. Fails on any non-2xx
/// status and on bodies larger than `MAX_RESPONSE_BYTES`.
///
/// Errors never carry the request URL: its query holds the user's
/// coordinates or city, and these errors end up in the warn-level log.
async fn get_text(url: reqwest::Url) -> anyhow::Result<String> {
    let client = HTTP_CLIENT
        .as_ref()
        .map_err(|e| anyhow::anyhow!("failed to build the HTTP client: {e}"))?;

    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(reqwest::Error::without_url)?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("unexpected HTTP status {status}");
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(reqwest::Error::without_url)?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            anyhow::bail!("response body exceeds {MAX_RESPONSE_BYTES} bytes");
        }
        body.extend_from_slice(&chunk);
    }

    Ok(String::from_utf8(body)?)
}

pub async fn fetch_location(location: &WeatherLocation, lang: &str) -> anyhow::Result<Location> {
    match location {
        WeatherLocation::City(city) => {
            let mut url = reqwest::Url::parse("https://geocoding-api.open-meteo.com/v1/search")?;
            url.query_pairs_mut()
                .append_pair("name", city)
                .append_pair("count", "1")
                .append_pair("language", lang)
                .append_pair("format", "json");

            let raw_data = get_text(url).await?;

            let data: GeoLocations = serde_json::from_str(&raw_data)?;

            data.results
                .first()
                .ok_or_else(|| anyhow::anyhow!("No location found"))
                .cloned()
                .map(|l| l.into())
        }
        WeatherLocation::Current => {
            let raw_data = get_text(reqwest::Url::parse("https://ipwho.is/")?).await?;

            let data: IpLocation = serde_json::from_str(&raw_data)?;

            data.into_location()
        }
        WeatherLocation::Coordinates(lat, lon) => {
            let (city, region_name) = match try_reverse_geocode(*lat, *lon, lang).await {
                Ok(Some((city, region))) => (city, region),
                _ => (format!("Lat: {}, Lon: {}", lat, lon), String::new()),
            };

            Ok(Location {
                latitude: *lat,
                longitude: *lon,
                city,
                region_name,
            })
        }
    }
}

async fn try_reverse_geocode(
    lat: f32,
    lon: f32,
    lang: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let mut url = reqwest::Url::parse("https://nominatim.openstreetmap.org/reverse")?;
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("lat", &lat.to_string())
        .append_pair("lon", &lon.to_string())
        .append_pair("accept-language", lang);

    let raw_data = get_text(url).await?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_data)
        && let Some(address) = json.get("address")
    {
        let mut city = None;

        if let Some(c) = address.get("city").and_then(|v| v.as_str()) {
            city = Some(c);
        } else if let Some(t) = address.get("town").and_then(|v| v.as_str()) {
            city = Some(t);
        } else if let Some(v) = address.get("village").and_then(|v| v.as_str()) {
            city = Some(v);
        } else if let Some(h) = address.get("hamlet").and_then(|v| v.as_str()) {
            city = Some(h);
        }

        if let Some(country) = address.get("country").and_then(|v| v.as_str())
            && let Some(city_name) = city
        {
            return Ok(Some((
                city_name.to_string(),
                if city_name != country {
                    country.to_string()
                } else {
                    String::new()
                },
            )));
        }

        if let Some(city_name) = city {
            return Ok(Some((city_name.to_string(), String::new())));
        }
    }

    Ok(None)
}

pub async fn fetch_weather_data(
    lat: f32,
    lon: f32,
    units: UnitSystem,
    wind_unit: crate::config::WindSpeedUnit,
) -> anyhow::Result<WeatherData> {
    let temp_param = match units {
        UnitSystem::Metric => "celsius",
        UnitSystem::Imperial => "fahrenheit",
    };
    let wind_param = wind_unit.api_param();

    let mut url = reqwest::Url::parse("https://api.open-meteo.com/v1/forecast")?;
    url.query_pairs_mut()
        .append_pair("latitude", &lat.to_string())
        .append_pair("longitude", &lon.to_string())
        .append_pair(
            "current",
            "weather_code,apparent_temperature,relative_humidity_2m,temperature_2m,is_day,wind_speed_10m,wind_direction_10m",
        )
        .append_pair("hourly", "weather_code,temperature_2m,is_day")
        .append_pair(
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,wind_speed_10m_max,wind_direction_10m_dominant,relative_humidity_2m_mean",
        )
        .append_pair("forecast_days", "7")
        .append_pair("temperature_unit", temp_param)
        .append_pair("wind_speed_unit", wind_param)
        .append_pair("timezone", "UTC");

    let raw_data = get_text(url).await?;

    let data: WeatherData = serde_json::from_str(&raw_data)?;

    Ok(data)
}

#[derive(Clone, Debug, Deserialize)]
pub struct WeatherData {
    pub current: WeatherCondition,
    pub hourly: HourlyWeatherData,
    pub daily: DailyWeatherData,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WeatherCondition {
    #[serde(with = "offsetdatetime_no_seconds")]
    pub time: NaiveDateTime,
    pub weather_code: u32,
    pub temperature_2m: f32,
    pub apparent_temperature: f32,
    pub relative_humidity_2m: u32,
    pub wind_speed_10m: f32,
    pub wind_direction_10m: u32,
    pub is_day: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HourlyWeatherData {
    #[serde(deserialize_with = "deserialize_datetime_vec")]
    pub time: Vec<NaiveDateTime>,
    pub weather_code: Vec<u32>,
    pub temperature_2m: Vec<f32>,
    pub is_day: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DailyWeatherData {
    #[serde(deserialize_with = "deserialize_date_vec")]
    pub time: Vec<NaiveDate>,
    pub weather_code: Vec<u32>,
    pub temperature_2m_max: Vec<f32>,
    pub temperature_2m_min: Vec<f32>,
    pub wind_speed_10m_max: Vec<f32>,
    pub wind_direction_10m_dominant: Vec<u32>,
    pub relative_humidity_2m_mean: Vec<u32>,
}

fn deserialize_datetime_vec<'de, D>(d: D) -> Result<Vec<NaiveDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let strs = Vec::<String>::deserialize(d)?;
    strs.into_iter()
        .map(|s| offsetdatetime_no_seconds::parse_str::<D>(&s))
        .collect()
}

fn deserialize_date_vec<'de, D>(d: D) -> Result<Vec<NaiveDate>, D::Error>
where
    D: Deserializer<'de>,
{
    let strs = Vec::<String>::deserialize(d)?;
    strs.into_iter()
        .map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom))
        .collect()
}

mod offsetdatetime_no_seconds {
    use chrono::NaiveDateTime;
    use serde::{Deserialize, Deserializer};

    pub fn parse_str<'de, D: Deserializer<'de>>(s: &str) -> Result<NaiveDateTime, D::Error> {
        let naive = NaiveDateTime::parse_from_str(s, "%FT%R").map_err(serde::de::Error::custom)?;

        Ok(naive)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<NaiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        parse_str::<D>(&s)
    }
}

fn weather_icon_bytes(code: u32, is_day: bool) -> &'static [u8] {
    match (code, is_day) {
        (0, true) => include_bytes!("../../../assets/weather_icon/clear-day.svg"),
        (0, false) => include_bytes!("../../../assets/weather_icon/clear-night.svg"),
        (1, true) => include_bytes!("../../../assets/weather_icon/cloudy-1-day.svg"),
        (1, false) => include_bytes!("../../../assets/weather_icon/cloudy-1-night.svg"),
        (2, true) => include_bytes!("../../../assets/weather_icon/cloudy-3-day.svg"),
        (2, false) => include_bytes!("../../../assets/weather_icon/cloudy-3-night.svg"),
        (3, _) => include_bytes!("../../../assets/weather_icon/cloudy.svg"),
        (45, _) | (48, _) => include_bytes!("../../../assets/weather_icon/fog.svg"),
        (51, true) => include_bytes!("../../../assets/weather_icon/rainy-1-day.svg"),
        (51, false) => include_bytes!("../../../assets/weather_icon/rainy-1-night.svg"),
        (53, true) | (56, true) => include_bytes!("../../../assets/weather_icon/rainy-2-day.svg"),
        (53, false) | (56, false) => {
            include_bytes!("../../../assets/weather_icon/rainy-2-night.svg")
        }
        (55, true) | (57, true) => include_bytes!("../../../assets/weather_icon/rainy-3-day.svg"),
        (55, false) | (57, false) => {
            include_bytes!("../../../assets/weather_icon/rainy-3-night.svg")
        }
        (61, _) => include_bytes!("../../../assets/weather_icon/rainy-1.svg"),
        (63, _) | (66, _) => include_bytes!("../../../assets/weather_icon/rainy-2.svg"),
        (65, _) | (67, _) => include_bytes!("../../../assets/weather_icon/rainy-3.svg"),
        (71, _) | (77, _) => include_bytes!("../../../assets/weather_icon/snowy-1.svg"),
        (73, _) => include_bytes!("../../../assets/weather_icon/snowy-2.svg"),
        (75, _) => include_bytes!("../../../assets/weather_icon/snowy-3.svg"),
        (80, true) => include_bytes!("../../../assets/weather_icon/showers-rainy-1-day.svg"),
        (80, false) => include_bytes!("../../../assets/weather_icon/showers-rainy-1-night.svg"),
        (81, true) => include_bytes!("../../../assets/weather_icon/showers-rainy-2-day.svg"),
        (81, false) => include_bytes!("../../../assets/weather_icon/showers-rainy-2-night.svg"),
        (82, true) => include_bytes!("../../../assets/weather_icon/showers-rainy-3-day.svg"),
        (82, false) => include_bytes!("../../../assets/weather_icon/showers-rainy-3-night.svg"),
        (85, true) => include_bytes!("../../../assets/weather_icon/snowy-2-day.svg"),
        (85, false) => include_bytes!("../../../assets/weather_icon/snowy-2-night.svg"),
        (86, true) => include_bytes!("../../../assets/weather_icon/snowy-3-day.svg"),
        (86, false) => include_bytes!("../../../assets/weather_icon/snowy-3-night.svg"),
        (95, _) => include_bytes!("../../../assets/weather_icon/isolated-thunderstorms.svg"),
        (96, _) => include_bytes!("../../../assets/weather_icon/scattered-thunderstorms.svg"),
        (99, _) => include_bytes!("../../../assets/weather_icon/severe-thunderstorm.svg"),
        _ => include_bytes!("../../../assets/weather_icon/unknown.svg"),
    }
}

pub fn weather_icon<'a>(code: u32, is_day: bool) -> Svg<'a> {
    svg(Handle::from_memory(weather_icon_bytes(code, is_day)))
}

pub fn weather_description(code: u32) -> String {
    match code {
        0 => t!("weather-clear-sky"),
        1 => t!("weather-mainly-clear"),
        2 => t!("weather-partly-cloudy"),
        3 => t!("weather-overcast"),
        45 => t!("weather-fog"),
        48 => t!("weather-fog-rime"),
        51 => t!("weather-drizzle-light"),
        53 => t!("weather-drizzle-moderate"),
        55 => t!("weather-drizzle-dense"),
        56 => t!("weather-drizzle-freezing-light"),
        57 => t!("weather-drizzle-freezing-dense"),
        61 => t!("weather-rain-slight"),
        63 => t!("weather-rain-moderate"),
        65 => t!("weather-rain-heavy"),
        66 => t!("weather-rain-freezing-light"),
        67 => t!("weather-rain-freezing-heavy"),
        71 => t!("weather-snow-slight"),
        73 => t!("weather-snow-moderate"),
        75 => t!("weather-snow-heavy"),
        77 => t!("weather-snow-grains"),
        80 => t!("weather-rain-showers-slight"),
        81 => t!("weather-rain-showers-moderate"),
        82 => t!("weather-rain-showers-violent"),
        85 => t!("weather-snow-showers-slight"),
        86 => t!("weather-snow-showers-heavy"),
        95 => t!("weather-thunderstorm"),
        96 => t!("weather-thunderstorm-hail-slight"),
        99 => t!("weather-thunderstorm-hail-heavy"),
        _ => t!("weather-unknown"),
    }
}
