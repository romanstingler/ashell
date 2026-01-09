use crate::{
    components::icons::{StaticIcon, icon},
    modules::system_info_components::{SharedSystemInfoService, TemperatureData},
    theme::AshellTheme,
};
use iced::{
    Alignment, Element, Length, Subscription, Theme,
    time::every,
    widget::{Column, column, container, horizontal_rule, row, text},
};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Message {
    Update,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TemperatureModuleConfig {
    pub warn_threshold: i32,
    pub alert_threshold: i32,
    pub sensor: String,
    pub format: TemperatureFormat,
    pub custom_name: Option<String>,
}

impl Default for TemperatureModuleConfig {
    fn default() -> Self {
        Self {
            warn_threshold: 60,
            alert_threshold: 80,
            sensor: "k10temp Tctl".to_string(),
            format: TemperatureFormat::IconAndValue,
            custom_name: None,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub enum TemperatureFormat {
    Icon,
    Value,
    IconAndValue,
}

pub struct TemperatureModule {
    config: TemperatureModuleConfig,
    service: SharedSystemInfoService,
}

impl TemperatureModule {
    pub fn new(config: TemperatureModuleConfig, service: SharedSystemInfoService) -> Self {
        Self { config, service }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Update => {
                if let Ok(mut service) = self.service.lock() {
                    service.update();
                }
            }
        }
    }

    fn format_display_text(&self, temperature_data: &TemperatureData) -> String {
        match temperature_data.temperature {
            Some(temp) => format!("{}°C", temp),
            None => "N/A".to_string(),
        }
    }

    fn info_element<'a>(
        theme: &AshellTheme,
        info_icon: StaticIcon,
        label: String,
        value: String,
    ) -> Element<'a, Message> {
        row!(
            container(icon(info_icon).size(theme.font_size.xl))
                .center_x(Length::Fixed(theme.space.xl as f32)),
            text(label).width(Length::Fill),
            text(value)
        )
        .align_y(Alignment::Center)
        .spacing(theme.space.xs)
        .into()
    }

    pub fn menu_view(&'_ self, theme: &AshellTheme) -> Element<'_, Message> {
        let temperature_data = if let Ok(service) = self.service.lock() {
            service.get_temperature_data().clone()
        } else {
            TemperatureData {
                temperature: None,
                sensor: self.config.sensor.clone(),
            }
        };

        let name = self.config.custom_name.as_deref().unwrap_or("Temperature");

        column!(
            text(format!("{} Info", name)).size(theme.font_size.lg),
            horizontal_rule(1),
            Column::new()
                .push(Self::info_element(
                    theme,
                    StaticIcon::Temp,
                    format!("{} Sensor", name),
                    temperature_data.sensor.clone(),
                ))
                .push(Self::info_element(
                    theme,
                    StaticIcon::Temp,
                    format!("{} Reading", name),
                    self.format_display_text(&temperature_data),
                ))
                .spacing(theme.space.xxs)
                .padding([0, theme.space.xs])
        )
        .spacing(theme.space.xs)
        .into()
    }

    pub fn view(&'_ self, theme: &AshellTheme) -> Element<'_, Message> {
        let temperature_data = if let Ok(service) = self.service.lock() {
            service.get_temperature_data().clone()
        } else {
            TemperatureData {
                temperature: None,
                sensor: self.config.sensor.clone(),
            }
        };

        let display_text = self.format_display_text(&temperature_data);

        let element = match self.config.format {
            TemperatureFormat::Icon => container(icon(StaticIcon::Temp)).into(),
            TemperatureFormat::Value => container(text(display_text)).into(),
            TemperatureFormat::IconAndValue => {
                container(row!(icon(StaticIcon::Temp), text(display_text)).spacing(theme.space.xxs))
                    .into()
            }
        };

        // Apply warning/alert styling
        if let (Some(temp), Some((warn_threshold, alert_threshold))) = (
            temperature_data.temperature,
            Some((self.config.warn_threshold, self.config.alert_threshold)),
        ) {
            container(element)
                .style(move |theme: &Theme| container::Style {
                    text_color: if temp > warn_threshold && temp < alert_threshold {
                        Some(theme.extended_palette().danger.weak.color)
                    } else if temp >= alert_threshold {
                        Some(theme.palette().danger)
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .into()
        } else {
            element
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        every(Duration::from_secs(5)).map(|_| Message::Update)
    }
}
