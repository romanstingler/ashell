use crate::{
    components::icons::{StaticIcon, icon},
    modules::system_info_components::{CpuData, SharedSystemInfoService, TemperatureData},
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
pub struct SystemInfoNewConfig {
    pub cpu: CpuModuleConfig,
    pub temperature: TemperatureModuleConfig,
}

impl Default for SystemInfoNewConfig {
    fn default() -> Self {
        Self {
            cpu: CpuModuleConfig::default(),
            temperature: TemperatureModuleConfig::default(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct CpuModuleConfig {
    pub warn_threshold: u32,
    pub alert_threshold: u32,
    pub format: CpuFormat,
    pub metrics: CpuMetrics,
    pub frequency_unit: FrequencyUnit,
    pub custom_name: Option<String>,
}

impl Default for CpuModuleConfig {
    fn default() -> Self {
        Self {
            warn_threshold: 60,
            alert_threshold: 80,
            format: CpuFormat::IconAndPercentage,
            metrics: CpuMetrics::Usage,
            frequency_unit: FrequencyUnit::GHz,
            custom_name: None,
        }
    }
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
pub enum CpuFormat {
    Icon,
    Percentage,
    IconAndPercentage,
}

#[derive(Deserialize, Clone, Debug)]
pub enum CpuMetrics {
    Usage,
    UsageAndFrequency,
    AllFrequencies,
}

#[derive(Deserialize, Clone, Debug)]
pub enum FrequencyUnit {
    KHz,
    MHz,
    GHz,
}

#[derive(Deserialize, Clone, Debug)]
pub enum TemperatureFormat {
    Icon,
    Value,
    IconAndValue,
}

pub struct SystemInfoNew {
    config: SystemInfoNewConfig,
    service: SharedSystemInfoService,
}

impl SystemInfoNew {
    pub fn new(config: SystemInfoNewConfig, service: SharedSystemInfoService) -> Self {
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

    fn format_cpu_frequency(&self, frequency: u64) -> String {
        match self.config.cpu.frequency_unit {
            FrequencyUnit::KHz => format!("{} kHz", frequency * 1000),
            FrequencyUnit::MHz => format!("{} MHz", frequency),
            FrequencyUnit::GHz => format!("{} GHz", frequency as f64 / 1000.0),
        }
    }

    fn format_cpu_display_text(&self, cpu_data: &CpuData) -> String {
        match self.config.cpu.metrics {
            CpuMetrics::Usage => format!("{}%", cpu_data.usage),
            CpuMetrics::UsageAndFrequency => {
                format!(
                    "{}% @ {}",
                    cpu_data.usage,
                    self.format_cpu_frequency(cpu_data.avg_frequency)
                )
            }
            CpuMetrics::AllFrequencies => {
                format!(
                    "{}% @ {}/{}/{}",
                    cpu_data.usage,
                    self.format_cpu_frequency(cpu_data.min_frequency),
                    self.format_cpu_frequency(cpu_data.avg_frequency),
                    self.format_cpu_frequency(cpu_data.max_frequency)
                )
            }
        }
    }

    fn format_temperature_display_text(&self, temperature_data: &TemperatureData) -> String {
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
        let cpu_data = if let Ok(service) = self.service.lock() {
            service.get_cpu_data().clone()
        } else {
            CpuData {
                usage: 0,
                avg_frequency: 0,
                min_frequency: 0,
                max_frequency: 0,
            }
        };

        let temperature_data = if let Ok(service) = self.service.lock() {
            service.get_temperature_data().clone()
        } else {
            TemperatureData {
                temperature: None,
                sensor: self.config.temperature.sensor.clone(),
            }
        };

        column!(
            text("System Info").size(theme.font_size.lg),
            horizontal_rule(1),
            Column::new()
                .push(Self::info_element(
                    theme,
                    StaticIcon::Cpu,
                    "CPU Usage".to_string(),
                    format!("{}%", cpu_data.usage),
                ))
                .push(Self::info_element(
                    theme,
                    StaticIcon::Cpu,
                    "CPU Frequency".to_string(),
                    self.format_cpu_frequency(cpu_data.avg_frequency),
                ))
                .push(Self::info_element(
                    theme,
                    StaticIcon::Temp,
                    format!(
                        "{} Sensor",
                        self.config
                            .temperature
                            .custom_name
                            .as_deref()
                            .unwrap_or("Temperature")
                    ),
                    temperature_data.sensor.clone(),
                ))
                .push(Self::info_element(
                    theme,
                    StaticIcon::Temp,
                    format!(
                        "{} Reading",
                        self.config
                            .temperature
                            .custom_name
                            .as_deref()
                            .unwrap_or("Temperature")
                    ),
                    self.format_temperature_display_text(&temperature_data),
                ))
                .spacing(theme.space.xxs)
                .padding([0, theme.space.xs])
        )
        .spacing(theme.space.xs)
        .into()
    }

    pub fn view(&'_ self, theme: &AshellTheme) -> Element<'_, Message> {
        let cpu_data = if let Ok(service) = self.service.lock() {
            service.get_cpu_data().clone()
        } else {
            CpuData {
                usage: 0,
                avg_frequency: 0,
                min_frequency: 0,
                max_frequency: 0,
            }
        };

        let temperature_data = if let Ok(service) = self.service.lock() {
            service.get_temperature_data().clone()
        } else {
            TemperatureData {
                temperature: None,
                sensor: self.config.temperature.sensor.clone(),
            }
        };

        let cpu_display_text = self.format_cpu_display_text(&cpu_data);
        let temperature_display_text = self.format_temperature_display_text(&temperature_data);

        let cpu_element: Element<Message> = match self.config.cpu.format {
            CpuFormat::Icon => container(icon(StaticIcon::Cpu)).into(),
            CpuFormat::Percentage => container(text(cpu_display_text)).into(),
            CpuFormat::IconAndPercentage => container(
                row!(icon(StaticIcon::Cpu), text(cpu_display_text)).spacing(theme.space.xxs),
            )
            .into(),
        };

        let temperature_element: Element<Message> = match self.config.temperature.format {
            TemperatureFormat::Icon => container(icon(StaticIcon::Temp)).into(),
            TemperatureFormat::Value => container(text(temperature_display_text)).into(),
            TemperatureFormat::IconAndValue => container(
                row!(icon(StaticIcon::Temp), text(temperature_display_text))
                    .spacing(theme.space.xxs),
            )
            .into(),
        };

        // Apply warning/alert styling for CPU
        let cpu_element = if let Some((warn_threshold, alert_threshold)) = Some((
            self.config.cpu.warn_threshold,
            self.config.cpu.alert_threshold,
        )) {
            container(cpu_element)
                .style(move |theme: &Theme| container::Style {
                    text_color: if cpu_data.usage > warn_threshold
                        && cpu_data.usage < alert_threshold
                    {
                        Some(theme.extended_palette().danger.weak.color)
                    } else if cpu_data.usage >= alert_threshold {
                        Some(theme.palette().danger)
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .into()
        } else {
            cpu_element
        };

        // Apply warning/alert styling for Temperature
        let temperature_element = if let (Some(temp), Some((warn_threshold, alert_threshold))) = (
            temperature_data.temperature,
            Some((
                self.config.temperature.warn_threshold,
                self.config.temperature.alert_threshold,
            )),
        ) {
            container(temperature_element)
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
            temperature_element
        };

        // Combine both elements
        row!(cpu_element, temperature_element)
            .spacing(theme.space.xs)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        every(Duration::from_secs(5)).map(|_| Message::Update)
    }
}
