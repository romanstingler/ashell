use std::sync::{Arc, Mutex};
use sysinfo::{Components, System};

#[derive(Debug, Clone)]
pub struct CpuData {
    pub usage: u32,
    pub avg_frequency: u64,
    pub min_frequency: u64,
    pub max_frequency: u64,
}

#[derive(Debug, Clone)]
pub struct TemperatureData {
    pub temperature: Option<i32>,
    pub sensor: String,
}

#[derive(Debug, Clone)]
pub struct SystemInfoData {
    pub cpu: CpuData,
    pub temperature: TemperatureData,
}

pub struct SystemInfoService {
    system: System,
    components: Components,
    data: SystemInfoData,
}

impl SystemInfoService {
    pub fn new(temperature_sensor: String) -> Self {
        let mut system = System::new();
        let mut components = Components::new_with_refreshed_list();

        let data = Self::collect_data(&mut system, &mut components, temperature_sensor);

        Self {
            system,
            components,
            data,
        }
    }

    fn collect_data(
        system: &mut System,
        components: &mut Components,
        temperature_sensor: String,
    ) -> SystemInfoData {
        // Refresh all system data
        system.refresh_memory();
        system.refresh_cpu_specifics(sysinfo::CpuRefreshKind::everything());
        components.refresh(true);

        // CPU data
        let cpu_usage = system.global_cpu_usage().floor() as u32;

        let cpu_frequencies: Vec<u64> = system.cpus().iter().map(|cpu| cpu.frequency()).collect();
        let avg_frequency = if cpu_frequencies.is_empty() {
            0
        } else {
            cpu_frequencies.iter().sum::<u64>() / cpu_frequencies.len() as u64
        };

        let cpu_data = CpuData {
            usage: cpu_usage,
            avg_frequency: avg_frequency,
            min_frequency: cpu_frequencies.iter().min().copied().unwrap_or(0),
            max_frequency: cpu_frequencies.iter().max().copied().unwrap_or(0),
        };

        // Temperature data
        let temperature = components
            .iter()
            .find(|component| component.label().contains(&temperature_sensor))
            .and_then(|component| {
                if let Some(temp) = component.temperature() {
                    if temp.is_finite() && temp > 0.0 {
                        Some(temp as i32)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

        let temperature_data = TemperatureData {
            temperature,
            sensor: temperature_sensor,
        };

        SystemInfoData {
            cpu: cpu_data,
            temperature: temperature_data,
        }
    }

    pub fn update(&mut self) {
        self.data = Self::collect_data(
            &mut self.system,
            &mut self.components,
            self.data.temperature.sensor.clone(),
        );
    }

    pub fn get_cpu_data(&self) -> &CpuData {
        &self.data.cpu
    }

    pub fn get_temperature_data(&self) -> &TemperatureData {
        &self.data.temperature
    }
}

pub type SharedSystemInfoService = Arc<Mutex<SystemInfoService>>;
