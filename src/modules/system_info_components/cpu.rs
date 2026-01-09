use crate::{
    components::icons::{StaticIcon, icon},
    modules::system_info_components::{CpuData, SharedSystemInfoService},
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
pub enum CpuFormat {
    Icon,
    Percentage,
    IconAndPercentage,
}

#[derive(Deserialize, Clone, Debug)]
pub enum CpuMetrics {
    Usage,             // Just usage percentage
    UsageAndFrequency, // Usage + avg frequency
    AllFrequencies,    // Usage + avg/min/max frequencies
}

#[derive(Deserialize, Clone, Debug)]
pub enum FrequencyUnit {
    KHz,
    MHz,
    GHz,
}

pub struct CpuModule {
    config: CpuModuleConfig,
    service: SharedSystemInfoService,
}

impl CpuModule {
    pub fn new(config: CpuModuleConfig, service: SharedSystemInfoService) -> Self {
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

    fn format_frequency(&self, frequency: u64) -> String {
        match self.config.frequency_unit {
            FrequencyUnit::KHz => format!("{} kHz", frequency * 1000),
            FrequencyUnit::MHz => format!("{} MHz", frequency),
            FrequencyUnit::GHz => format!("{} GHz", frequency as f64 / 1000.0),
        }
    }

    fn format_display_text(&self, cpu_data: &CpuData) -> String {
        match self.config.metrics {
            CpuMetrics::Usage => format!("{}%", cpu_data.usage),
            CpuMetrics::UsageAndFrequency => {
                format!(
                    "{}% @ {}",
                    cpu_data.usage,
                    self.format_frequency(cpu_data.avg_frequency)
                )
            }
            CpuMetrics::AllFrequencies => {
                format!(
                    "{}% @ {}/{}/{}",
                    cpu_data.usage,
                    self.format_frequency(cpu_data.min_frequency),
                    self.format_frequency(cpu_data.avg_frequency),
                    self.format_frequency(cpu_data.max_frequency)
                )
            }
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

    fn indicator_info_element<'a, V: std::fmt::Display + PartialOrd + 'a>(
        theme: &AshellTheme,
        info_icon: StaticIcon,
        value: V,
        unit: &str,
        threshold: Option<(V, V)>,
        prefix: Option<&str>,
    ) -> Element<'a, Message> {
        let element = container(
            row!(
                icon(info_icon),
                if let Some(prefix) = prefix {
                    text(format!("{prefix} {value}{unit}"))
                } else {
                    text(format!("{value}{unit}"))
                }
            )
            .spacing(theme.space.xxs),
        );

        if let Some((warn_threshold, alert_threshold)) = threshold {
            element
                .style(move |theme: &Theme| container::Style {
                    text_color: if value > warn_threshold && value < alert_threshold {
                        Some(theme.extended_palette().danger.weak.color)
                    } else if value >= alert_threshold {
                        Some(theme.palette().danger)
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .into()
        } else {
            element.into()
        }
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

        let name = self.config.custom_name.as_deref().unwrap_or("CPU");

        column!(
            text(format!("{} Info", name)).size(theme.font_size.lg),
            horizontal_rule(1),
            Column::new()
                .push(Self::info_element(
                    theme,
                    StaticIcon::Cpu,
                    format!("{} Usage", name),
                    format!("{}%", cpu_data.usage),
                ))
                .push(Self::info_element(
                    theme,
                    StaticIcon::Cpu,
                    format!("{} Avg Frequency", name),
                    self.format_frequency(cpu_data.avg_frequency),
                ))
                .push_maybe(
                    if matches!(self.config.metrics, CpuMetrics::AllFrequencies) {
                        Some(Self::info_element(
                            theme,
                            StaticIcon::Cpu,
                            format!("{} Min Frequency", name),
                            self.format_frequency(cpu_data.min_frequency),
                        ))
                    } else {
                        None
                    }
                )
                .push_maybe(
                    if matches!(self.config.metrics, CpuMetrics::AllFrequencies) {
                        Some(Self::info_element(
                            theme,
                            StaticIcon::Cpu,
                            format!("{} Max Frequency", name),
                            self.format_frequency(cpu_data.max_frequency),
                        ))
                    } else {
                        None
                    }
                )
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

        let display_text = self.format_display_text(&cpu_data);

        let element = match self.config.format {
            CpuFormat::Icon => container(icon(StaticIcon::Cpu)).into(),
            CpuFormat::Percentage => container(text(display_text)).into(),
            CpuFormat::IconAndPercentage => {
                container(row!(icon(StaticIcon::Cpu), text(display_text)).spacing(theme.space.xxs))
                    .into()
            }
        };

        // Apply warning/alert styling
        if let Some((warn_threshold, alert_threshold)) =
            Some((self.config.warn_threshold, self.config.alert_threshold))
        {
            container(element)
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
            element
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        every(Duration::from_secs(5)).map(|_| Message::Update)
    }
}
