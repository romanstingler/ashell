use super::{backlight::BacklightBackend, ddc::DdcCiBackend};

/// Common interface implemented by the brightness backends.
pub trait BrightnessBackend: Send + Sync {
    /// Read the current brightness value.
    async fn get_brightness(&self) -> anyhow::Result<u32>;

    /// Read the maximum brightness value.
    async fn get_max_brightness(&self) -> anyhow::Result<u32>;

    /// Set the brightness value.
    async fn set_brightness(&self, value: u32) -> anyhow::Result<()>;
}

/// Brightness backend picked at service initialization.
#[derive(Debug, Clone)]
pub enum Backend {
    Backlight(BacklightBackend),
    DdcCi(DdcCiBackend),
}

impl BrightnessBackend for Backend {
    async fn get_brightness(&self) -> anyhow::Result<u32> {
        match self {
            Self::Backlight(backend) => backend.get_brightness().await,
            Self::DdcCi(backend) => backend.get_brightness().await,
        }
    }

    async fn get_max_brightness(&self) -> anyhow::Result<u32> {
        match self {
            Self::Backlight(backend) => backend.get_max_brightness().await,
            Self::DdcCi(backend) => backend.get_max_brightness().await,
        }
    }

    async fn set_brightness(&self, value: u32) -> anyhow::Result<()> {
        match self {
            Self::Backlight(backend) => backend.set_brightness(value).await,
            Self::DdcCi(backend) => backend.set_brightness(value).await,
        }
    }
}
