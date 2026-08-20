use super::backend::BrightnessBackend;
use libmonitor::Monitor;
use log::warn;
use std::fs::{self, OpenOptions};

/// Maximum value of the virtual brightness range exposed by the DDC/CI backend.
const MAX_BRIGHTNESS: u32 = 100;

/// Brightness backend that talks DDC/CI to external monitors over I2C.
///
/// DDC/CI provides no change notifications, so values are re-read on demand
/// (see `BrightnessService::sync_brightness`). The monitors are re-enumerated
/// on every operation, which keeps hot-plugged displays working without
/// holding `/dev/i2c-*` handles open.
#[derive(Debug, Clone, Copy)]
pub struct DdcCiBackend;

impl DdcCiBackend {
    pub async fn new() -> anyhow::Result<Self> {
        let usable = tokio::task::spawn_blocking(|| {
            // libmonitor opens the i2c buses with `unwrap()`, so missing
            // permissions would panic instead of returning an error.
            if let Err(err) = ensure_i2c_access() {
                warn!("{err}");
                return false;
            }

            Monitor::enumerate().any(|mut monitor| monitor.get_luminance().is_ok())
        })
        .await?;

        if usable {
            Ok(Self)
        } else {
            Err(anyhow::anyhow!("No usable DDC/CI monitors found"))
        }
    }
}

impl BrightnessBackend for DdcCiBackend {
    async fn get_brightness(&self) -> anyhow::Result<u32> {
        tokio::task::spawn_blocking(|| {
            let mut luminances = Vec::new();
            for mut monitor in Monitor::enumerate() {
                match monitor.get_luminance() {
                    Ok(luminance) => luminances.push(luminance),
                    Err(err) => warn!("Failed to read DDC/CI luminance: {err}"),
                }
            }

            if luminances.is_empty() {
                return Err(anyhow::anyhow!("No readable DDC/CI monitors found"));
            }

            let average = luminances.iter().sum::<f64>() / luminances.len() as f64;

            Ok((average * MAX_BRIGHTNESS as f64).round() as u32)
        })
        .await?
    }

    async fn get_max_brightness(&self) -> anyhow::Result<u32> {
        Ok(MAX_BRIGHTNESS)
    }

    async fn set_brightness(&self, value: u32) -> anyhow::Result<()> {
        let luminance = (value as f64 / MAX_BRIGHTNESS as f64).clamp(0.0, 1.0);

        tokio::task::spawn_blocking(move || {
            let mut monitors = 0;
            let mut failed = 0;
            for mut monitor in Monitor::enumerate() {
                monitors += 1;
                if let Err(err) = monitor.set_luminance(luminance) {
                    failed += 1;
                    warn!("Failed to set DDC/CI luminance: {err}");
                }
            }

            if monitors == 0 || failed == monitors {
                Err(anyhow::anyhow!("No DDC/CI monitors could be adjusted"))
            } else {
                Ok(())
            }
        })
        .await?
    }
}

/// Ensure that every `/dev/i2c-*` device can be opened read-write.
///
/// ashell runs as a regular user, while the i2c buses usually require the
/// user to be in the `i2c` group (or an equivalent udev rule). Verifying
/// access here keeps the backend from panicking on permission errors later.
fn ensure_i2c_access() -> anyhow::Result<()> {
    for entry in fs::read_dir("/dev")? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if !name.starts_with("i2c-") {
            continue;
        }

        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|err| anyhow::anyhow!("{} is not accessible: {err}", path.display()))?;
    }

    Ok(())
}
