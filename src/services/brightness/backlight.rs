use super::backend::BrightnessBackend;
use std::{fs, path::PathBuf};
use tokio::io::{Interest, unix::AsyncFd};
use zbus::proxy;

/// Brightness backend that talks to the kernel backlight interface exposed
/// under `/sys/class/backlight`, applying changes through logind.
#[derive(Debug, Clone)]
pub struct BacklightBackend {
    conn: zbus::Connection,
    device_path: PathBuf,
}

impl BacklightBackend {
    pub async fn new() -> anyhow::Result<Self> {
        let device = Self::enumerate()?
            .into_iter()
            .find(|device| device.subsystem().and_then(|s| s.to_str()) == Some("backlight"))
            .ok_or_else(|| anyhow::anyhow!("No backlight devices found"))?;

        let device_path = device.syspath().to_path_buf();
        let conn = zbus::Connection::system().await?;

        Ok(Self { conn, device_path })
    }

    fn enumerate() -> anyhow::Result<Vec<udev::Device>> {
        let mut enumerator = udev::Enumerator::new()?;
        enumerator.match_subsystem("backlight")?;

        Ok(enumerator.scan_devices()?.collect())
    }

    pub async fn monitor_listener() -> anyhow::Result<AsyncFd<udev::MonitorSocket>> {
        let socket = udev::MonitorBuilder::new()?
            .match_subsystem("backlight")?
            .listen()?;

        Ok(AsyncFd::with_interest(
            socket,
            Interest::READABLE | Interest::WRITABLE,
        )?)
    }
}

impl BrightnessBackend for BacklightBackend {
    async fn get_brightness(&self) -> anyhow::Result<u32> {
        let brightness = fs::read_to_string(self.device_path.join("brightness"))?;
        let brightness = brightness.trim().parse::<u32>()?;

        Ok(brightness)
    }

    async fn get_max_brightness(&self) -> anyhow::Result<u32> {
        let max_brightness = fs::read_to_string(self.device_path.join("max_brightness"))?;
        let max_brightness = max_brightness.trim().parse::<u32>()?;

        Ok(max_brightness)
    }

    async fn set_brightness(&self, value: u32) -> anyhow::Result<()> {
        let brightness_ctrl = BrightnessCtrlProxy::new(&self.conn).await?;
        let device_name = self
            .device_path
            .iter()
            .next_back()
            .and_then(|d| d.to_str())
            .unwrap_or_default();

        brightness_ctrl
            .set_brightness("backlight", device_name, value)
            .await?;

        Ok(())
    }
}

#[proxy(
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto",
    interface = "org.freedesktop.login1.Session"
)]
trait BrightnessCtrl {
    fn set_brightness(&self, subsystem: &str, name: &str, value: u32) -> zbus::Result<()>;
}
