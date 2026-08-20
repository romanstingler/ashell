mod backend;
mod backlight;
mod ddc;

use self::backend::{Backend, BrightnessBackend};
use super::{ReadOnlyService, Service, ServiceEvent};
use crate::{services::throttle::ThrottleExt, utils::remote_value::Remote};
use iced::{
    Subscription, Task,
    futures::{SinkExt, StreamExt, channel::mpsc::Sender, stream::pending},
    stream::channel,
};
use log::{debug, error, info, warn};
use std::{
    any::TypeId,
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug, Clone, Default)]
pub struct BrightnessData {
    pub current: Remote<u32>,
    pub max: u32,
}

#[derive(Debug, Clone)]
pub struct BrightnessService {
    data: BrightnessData,
    commander: UnboundedSender<BrightnessCommand>,
    backend: Backend,
}

impl Deref for BrightnessService {
    type Target = BrightnessData;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for BrightnessService {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl BrightnessService {
    /// Re-read the current brightness from the backend.
    ///
    /// Used to pick up changes made outside of ashell, since not every
    /// backend can report them passively.
    pub fn sync_brightness(&self) -> Task<ServiceEvent<Self>> {
        let backend = self.backend.clone();

        Task::perform(
            async move { backend.get_brightness().await },
            |result| match result {
                Ok(value) => ServiceEvent::Update(BrightnessEvent(value)),
                Err(err) => {
                    warn!("Failed to sync brightness: {err}");
                    ServiceEvent::Error(())
                }
            },
        )
    }

    async fn initialize_data(backend: &Backend) -> anyhow::Result<BrightnessData> {
        let max_brightness = backend.get_max_brightness().await?;
        let actual_brightness = backend.get_brightness().await?;

        Ok(BrightnessData {
            current: Remote::new(actual_brightness),
            max: max_brightness,
        })
    }

    async fn detect_backend() -> Option<Backend> {
        match backlight::BacklightBackend::new().await {
            Ok(backend) => {
                info!("Using the backlight brightness backend");
                Some(Backend::Backlight(backend))
            }
            Err(err) => {
                debug!("Backlight brightness backend unavailable: {err}");

                match ddc::DdcCiBackend::new().await {
                    Ok(backend) => {
                        info!("Using the DDC/CI brightness backend");
                        Some(Backend::DdcCi(backend))
                    }
                    Err(err) => {
                        warn!("DDC/CI brightness backend unavailable: {err}");
                        None
                    }
                }
            }
        }
    }

    fn start_commander(backend: Backend, to_server_rx: UnboundedReceiver<BrightnessCommand>) {
        tokio::spawn(async move {
            let mut stream =
                UnboundedReceiverStream::new(to_server_rx).throttle(Duration::from_millis(100));
            while let Some(cmd) = stream.next().await {
                if let Err(err) = backend.set_brightness(cmd.0).await {
                    warn!("Failed to set brightness: {err}");
                }
            }
        });
    }

    async fn start_listening(state: State, output: &mut Sender<ServiceEvent<Self>>) -> State {
        match state {
            State::Init => match Self::detect_backend().await {
                Some(backend) => match Self::initialize_data(&backend).await {
                    Ok(data) => {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                        Self::start_commander(backend.clone(), rx);
                        let _ = output
                            .send(ServiceEvent::Init(BrightnessService {
                                data,
                                commander: tx,
                                backend: backend.clone(),
                            }))
                            .await;

                        State::Active(backend)
                    }
                    Err(err) => {
                        error!("Failed to initialize brightness data: {err}");

                        State::Error
                    }
                },
                None => {
                    error!("No brightness devices found");

                    State::Error
                }
            },
            State::Active(backend) => match backend {
                Backend::Backlight(backend) => {
                    info!("Listening for brightness events");
                    let mut current_value = backend.get_brightness().await.unwrap_or_default();

                    match backlight::BacklightBackend::monitor_listener().await {
                        Ok(mut socket) => {
                            loop {
                                debug!("Waiting for brightness events");

                                match socket.writable_mut().await {
                                    Ok(mut socket) => {
                                        for evt in socket.get_inner().iter() {
                                            debug!("{:?}: {:?}", evt.event_type(), evt.device());

                                            if evt.device().subsystem().and_then(|s| s.to_str())
                                                == Some("backlight")
                                            {
                                                match evt.event_type() {
                                                    udev::EventType::Change => {
                                                        debug!(
                                                            "Changed backlight device: {:?}",
                                                            evt.syspath()
                                                        );
                                                        if let Ok(new_value) =
                                                            backend.get_brightness().await
                                                            && new_value != current_value
                                                        {
                                                            current_value = new_value;
                                                            let _ = output
                                                                .send(ServiceEvent::Update(
                                                                    BrightnessEvent(new_value),
                                                                ))
                                                                .await;
                                                        }

                                                        break;
                                                    }
                                                    _ => {
                                                        debug!(
                                                            "Unhandled event type: {:?}",
                                                            evt.event_type()
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        socket.clear_ready();
                                    }
                                    _ => {
                                        warn!("Failed to get writable socket");
                                        break;
                                    }
                                }
                            }
                            State::Active(Backend::Backlight(backend))
                        }
                        Err(err) => {
                            error!("Failed to listen for brightness events: {err}");

                            State::Error
                        }
                    }
                }
                Backend::DdcCi(backend) => {
                    // DDC/CI provides no change notifications: the brightness
                    // is re-read on demand instead (see `sync_brightness`).
                    debug!("The DDC/CI backend does not emit brightness events");

                    let _ = pending::<u8>().next().await;

                    State::Active(Backend::DdcCi(backend))
                }
            },
            State::Error => {
                error!("Brightness service error");

                let _ = pending::<u8>().next().await;
                State::Error
            }
        }
    }
}

enum State {
    Init,
    Active(Backend),
    Error,
}

#[derive(Debug, Clone)]
pub struct BrightnessEvent(u32);

impl ReadOnlyService for BrightnessService {
    type UpdateEvent = BrightnessEvent;
    type Error = ();

    fn update(&mut self, event: Self::UpdateEvent) {
        self.data.current.receive(event.0);
    }

    fn subscribe() -> Subscription<ServiceEvent<Self>> {
        Subscription::run_with(TypeId::of::<Self>(), |_| {
            channel(100, async |mut output| {
                let mut state = State::Init;

                loop {
                    state = BrightnessService::start_listening(state, &mut output).await;
                }
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct BrightnessCommand(pub u32);

impl Service for BrightnessService {
    type Command = BrightnessCommand;

    fn command(&mut self, command: Self::Command) -> Task<ServiceEvent<Self>> {
        let _ = self.commander.send(command);
        Task::none()
    }
}
