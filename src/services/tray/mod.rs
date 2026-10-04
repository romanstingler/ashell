use super::xdg_icons;
use super::{ReadOnlyService, Service, ServiceEvent};
use dbus::{
    DBusMenuProxy, Layout, StatusNotifierItemProxy, StatusNotifierWatcher,
    StatusNotifierWatcherProxy,
};
use iced::{
    Subscription, Task,
    futures::{
        FutureExt, SinkExt, StreamExt,
        channel::mpsc::Sender,
        future::BoxFuture,
        future::ready,
        stream::{self, AbortHandle, BoxStream, FuturesUnordered, SelectAll, abortable, pending},
    },
    stream::channel,
    widget::image,
};
use log::{debug, error, info, trace, warn};
use std::{any::TypeId, collections::HashMap, ops::Deref, path::Path, time::Duration};
use zbus::fdo::RequestNameFlags;

pub mod dbus;

pub type TrayIcon = super::xdg_icons::XdgIcon;

/// Delays before retrying an item that failed to load. Some clients call
/// `RegisterStatusNotifierItem` before exporting the object.
const ITEM_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

/// Upper bound for the backoff between host restarts after a failure.
const MAX_HOST_BACKOFF: Duration = Duration::from_secs(30);

fn pixmap_to_icon(icons: &[dbus::Icon]) -> Option<TrayIcon> {
    icons
        .iter()
        .filter(|i| {
            // SNI clients sometimes return entries with zero dimensions or a
            // bytes payload that doesn't match width*height*4 (e.g. when only
            // IconName is populated). Feeding those to iced's atlas uploader
            // panics in the padding loop, so drop them up front.
            if i.width <= 0 || i.height <= 0 {
                debug!(
                    "unable to convert pixmap to icon: invalid dimensions {}x{}",
                    i.width, i.height
                );
                return false;
            }
            let expected = (i.width as usize)
                .checked_mul(i.height as usize)
                .and_then(|v| v.checked_mul(4));

            if Some(i.bytes.len()) != expected {
                debug!(
                    "pixmap byte mismatch ({}x{} expected {:?} bytes, got {})",
                    i.width,
                    i.height,
                    expected,
                    i.bytes.len()
                );
                return false;
            }

            true
        })
        .max_by_key(|i| {
            trace!("tray icon w {}, h {}", i.width, i.height);
            (i.width, i.height)
        })
        .map(|i| {
            // Convert ARGB to RGBA
            let mut bytes = i.bytes.clone();
            for pixel in bytes.as_chunks_mut::<4>().0 {
                pixel.rotate_left(1);
            }
            TrayIcon::Image(image::Handle::from_rgba(
                i.width as u32,
                i.height as u32,
                bytes,
            ))
        })
}

/// The icon properties of an item, read together so one refresh resolves
/// one consistent state.
#[derive(Clone, Default, PartialEq, Eq)]
struct IconSource {
    name: String,
    theme_path: String,
    pixmap: Vec<dbus::Icon>,
}

impl std::fmt::Debug for IconSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconSource")
            .field("name", &self.name)
            .field("theme_path", &self.theme_path)
            .field(
                "pixmap",
                &self
                    .pixmap
                    .iter()
                    .map(|i| (i.width, i.height))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl IconSource {
    /// `None` when neither `IconName` nor `IconPixmap` could be read, e.g.
    /// because the item is going away.
    async fn read(proxy: &StatusNotifierItemProxy<'_>) -> Option<Self> {
        let (name, pixmap, theme_path) = iced::futures::join!(
            proxy.icon_name(),
            proxy.icon_pixmap(),
            proxy.icon_theme_path()
        );
        if name.is_err() && pixmap.is_err() {
            return None;
        }

        Some(Self {
            name: name.unwrap_or_default(),
            theme_path: theme_path.unwrap_or_default(),
            pixmap: pixmap.unwrap_or_default(),
        })
    }

    /// Named icons win over pixmaps, as the SNI spec asks and as Waybar and
    /// noctalia do. Fuzzy name matching runs last so it cannot replace a
    /// correct pixmap with an unrelated icon.
    fn resolve(&self) -> Option<TrayIcon> {
        let name = self.name.as_str();

        if name.starts_with('/') {
            if let Some(icon) = xdg_icons::get_icon_from_file(Path::new(name)) {
                return Some(icon);
            }
        } else if !name.is_empty() {
            if !self.theme_path.is_empty()
                && let Some(icon) =
                    xdg_icons::get_icon_from_theme_path(Path::new(&self.theme_path), name)
            {
                return Some(icon);
            }
            if let Some(icon) = xdg_icons::get_exact_icon_from_name(name) {
                return Some(icon);
            }
        }

        if let Some(icon) = pixmap_to_icon(&self.pixmap) {
            return Some(icon);
        }

        if !name.starts_with('/') {
            let icon = xdg_icons::get_icon_from_name(name);
            if icon.is_none() {
                debug!("tray icon {self:?} did not resolve");
            }
            return icon;
        }

        debug!("tray icon {self:?} did not resolve");
        None
    }
}

/// Coalesces bursts of icon notifications, like Plasma and Waybar do.
const ICON_REFRESH_DEBOUNCE: Duration = Duration::from_millis(100);

const ICON_PROPERTIES: [&str; 3] = ["IconName", "IconPixmap", "IconThemePath"];

struct IconRefresh {
    triggers: SelectAll<BoxStream<'static, ()>>,
    proxy: StatusNotifierItemProxy<'static>,
    last: IconSource,
    check_now: bool,
}

fn split_service_name(name: &str) -> (&str, &str) {
    match name.find('/') {
        Some(idx) => (&name[..idx], &name[idx..]),
        None => (name, "/StatusNotifierItem"),
    }
}

/// The object answered but has no such property. Qt and GDBus reply
/// `InvalidArgs`, zbus replies `UnknownProperty`.
fn is_missing_property(err: &zbus::Error) -> bool {
    matches!(
        err,
        zbus::Error::FDO(e) if matches!(
            **e,
            zbus::fdo::Error::UnknownProperty(_) | zbus::fdo::Error::InvalidArgs(_)
        )
    )
}

/// Menu paths used by clients to say they have no dbusmenu.
fn is_no_menu_path(path: &str) -> bool {
    path.is_empty() || path == "/" || path == "/NO_DBUSMENU"
}

#[derive(Debug, Clone)]
pub enum TrayEvent {
    Registered(StatusNotifierItem),
    /// `None` when nothing resolves; the module shows a placeholder.
    IconChanged(String, Option<TrayIcon>),
    MenuLayoutChanged(String, Layout),
    Unregistered(String),
    None,
}

#[derive(Debug, Clone)]
pub struct StatusNotifierItem {
    pub name: String,
    pub icon: Option<TrayIcon>,
    /// `None` until the layout is known, or when the item has no menu.
    pub menu: Option<Layout>,
    icon_source: IconSource,
    item_proxy: StatusNotifierItemProxy<'static>,
    menu_proxy: Option<DBusMenuProxy<'static>>,
}

impl StatusNotifierItem {
    pub async fn new(conn: &zbus::Connection, name: String) -> anyhow::Result<Self> {
        let (dest, path) = split_service_name(&name);

        let item_proxy = StatusNotifierItemProxy::builder(conn)
            .destination(dest.to_owned())?
            .path(path.to_owned())?
            .build()
            .await?;

        debug!("item_proxy {item_proxy:?}");

        // `Menu` is optional in the SNI spec. Any other error means the object
        // is not there (yet), which the caller retries.
        let menu_path = match item_proxy.menu().await {
            Ok(path) if !is_no_menu_path(path.as_str()) => Some(path),
            Ok(_) => None,
            Err(err) if is_missing_property(&err) => None,
            Err(err) => return Err(err.into()),
        };

        let icon_source = IconSource::read(&item_proxy).await.unwrap_or_default();
        let icon = icon_source.resolve();

        let menu_proxy = match menu_path {
            Some(menu_path) => Some(
                DBusMenuProxy::builder(conn)
                    .destination(dest.to_owned())?
                    .path(menu_path)?
                    .build()
                    .await?,
            ),
            None => {
                debug!("tray item {name} has no menu");
                None
            }
        };

        let menu = match &menu_proxy {
            Some(menu_proxy) => match menu_proxy.get_layout(0, -1, &[]).await {
                Ok((_, layout)) => Some(layout),
                Err(err) => {
                    // A later LayoutUpdated fills it in.
                    debug!("tray item {name}: menu layout unavailable: {err}");
                    None
                }
            },
            None => None,
        };

        Ok(Self {
            name,
            icon,
            menu,
            icon_source,
            item_proxy,
            menu_proxy,
        })
    }

    pub fn has_menu(&self) -> bool {
        self.menu_proxy.is_some()
    }

    /// Change notifications for this item, merged into one stream.
    async fn events(&self, conn: &zbus::Connection) -> BoxStream<'static, TrayEvent> {
        let name = self.name.clone();
        let mut streams: Vec<BoxStream<'static, TrayEvent>> = Vec::with_capacity(2);

        match self.icon_events(conn).await {
            Ok(stream) => streams.push(stream),
            Err(err) => debug!("tray item {name}: no icon subscription: {err}"),
        }

        if let Some(menu_proxy) = &self.menu_proxy {
            match menu_proxy.receive_layout_updated().await {
                Ok(layout_updated) => streams.push(
                    layout_updated
                        .filter_map({
                            let name = name.clone();
                            let menu_proxy = menu_proxy.clone();
                            move |_| {
                                debug!("layout update event name {name}");

                                let name = name.clone();
                                let menu_proxy = menu_proxy.clone();
                                async move {
                                    menu_proxy.get_layout(0, -1, &[]).await.ok().map(
                                        |(_, layout)| TrayEvent::MenuLayoutChanged(name, layout),
                                    )
                                }
                            }
                        })
                        .boxed(),
                ),
                Err(err) => debug!("tray item {name}: no LayoutUpdated subscription: {err}"),
            }
        }

        iced::futures::stream::select_all(streams).boxed()
    }

    /// One `IconChanged` per change of the icon properties, whichever way
    /// the client announces it: `NewIcon`, `NewIconThemePath` or
    /// `PropertiesChanged`.
    async fn icon_events(
        &self,
        conn: &zbus::Connection,
    ) -> anyhow::Result<BoxStream<'static, TrayEvent>> {
        let (dest, path) = split_service_name(&self.name);
        // NewIcon has no matching PropertiesChanged, so a cached read would be stale.
        let proxy = StatusNotifierItemProxy::builder(conn)
            .destination(dest.to_owned())?
            .path(path.to_owned())?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        let properties = zbus::fdo::PropertiesProxy::builder(conn)
            .destination(dest.to_owned())?
            .path(path.to_owned())?
            .build()
            .await?;

        let mut triggers = SelectAll::new();
        triggers.push(
            self.item_proxy
                .receive_new_icon()
                .await?
                .map(|_| ())
                .boxed(),
        );
        triggers.push(
            self.item_proxy
                .receive_new_icon_theme_path()
                .await?
                .map(|_| ())
                .boxed(),
        );
        triggers.push(
            properties
                .receive_properties_changed()
                .await?
                .filter(|signal| {
                    ready(signal.args().is_ok_and(|args| {
                        args.interface_name == "org.kde.StatusNotifierItem"
                            && args
                                .changed_properties
                                .keys()
                                .chain(args.invalidated_properties.iter())
                                .any(|prop| ICON_PROPERTIES.contains(prop))
                    }))
                })
                .map(|_| ())
                .boxed(),
        );

        // Subscribed first, then compared with what `new` resolved, so a change
        // in between is not lost.
        let state = IconRefresh {
            triggers,
            proxy,
            last: self.icon_source.clone(),
            check_now: true,
        };
        let name = self.name.clone();

        Ok(stream::unfold(state, move |mut state| {
            let name = name.clone();
            async move {
                loop {
                    if !std::mem::take(&mut state.check_now) {
                        state.triggers.next().await?;
                        tokio::time::sleep(ICON_REFRESH_DEBOUNCE).await;
                        while let Some(Some(())) = state.triggers.next().now_or_never() {}
                    }

                    let Some(source) = IconSource::read(&state.proxy).await else {
                        continue;
                    };
                    if source == state.last {
                        continue;
                    }

                    debug!("tray item {name}: icon source {source:?}");
                    let icon = source.resolve();
                    state.last = source;
                    return Some((TrayEvent::IconChanged(name, icon), state));
                }
            }
        })
        .boxed())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TrayData(Vec<StatusNotifierItem>);

impl Deref for TrayData {
    type Target = Vec<StatusNotifierItem>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct TrayService {
    pub data: TrayData,
    _conn: zbus::Connection,
}

impl Deref for TrayService {
    type Target = TrayData;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

enum State {
    Init,
    Active {
        conn: zbus::Connection,
        /// Our `org.kde.StatusNotifierHost-<pid>` name, if we got it.
        host: Option<String>,
        failures: u32,
    },
    Error,
}

/// Per-item subscriptions of the host, keyed by service name so they can be
/// dropped when the item goes away.
#[derive(Default)]
struct ItemStreams {
    events: SelectAll<BoxStream<'static, TrayEvent>>,
    handles: HashMap<String, AbortHandle>,
}

impl ItemStreams {
    async fn add(&mut self, conn: &zbus::Connection, item: &StatusNotifierItem) {
        self.remove(&item.name);

        let (events, handle) = abortable(item.events(conn).await);
        self.events.push(events.boxed());
        self.handles.insert(item.name.clone(), handle);
    }

    fn remove(&mut self, name: &str) {
        if let Some(handle) = self.handles.remove(name) {
            handle.abort();
        }
    }

    fn contains(&self, name: &str) -> bool {
        self.handles.contains_key(name)
    }
}

type RetryQueue = FuturesUnordered<BoxFuture<'static, (String, usize)>>;

/// Queues another attempt for `name`. Returns `false` once the retries are
/// used up.
fn schedule_retry(retries: &mut RetryQueue, name: String, attempt: usize) -> bool {
    let Some(&delay) = ITEM_RETRY_DELAYS.get(attempt) else {
        return false;
    };

    retries.push(
        async move {
            tokio::time::sleep(delay).await;
            (name, attempt + 1)
        }
        .boxed(),
    );

    true
}

impl TrayService {
    /// Loads an item and subscribes to its changes, or queues a retry.
    async fn load_item(
        conn: &zbus::Connection,
        name: String,
        attempt: usize,
        streams: &mut ItemStreams,
        retries: &mut RetryQueue,
    ) -> Option<StatusNotifierItem> {
        match StatusNotifierItem::new(conn, name.clone()).await {
            Ok(item) => {
                streams.add(conn, &item).await;
                Some(item)
            }
            Err(err) => {
                if schedule_retry(retries, name.clone(), attempt) {
                    debug!("tray item {name} not ready (attempt {attempt}): {err}");
                } else {
                    warn!("Giving up on tray item {name}: {err}");
                }
                None
            }
        }
    }

    /// Runs the host side: reads the watcher's items, sends `Init`, then
    /// forwards changes. Returns `Ok` when the watcher changes owner, so the
    /// caller starts over against the new one, and `Err` when setup fails or a
    /// signal stream ends.
    async fn run_host(
        conn: &zbus::Connection,
        host: Option<&str>,
        output: &mut Sender<ServiceEvent<Self>>,
        failures: &mut u32,
    ) -> anyhow::Result<()> {
        // Our watcher never emits PropertiesChanged for RegisteredStatusNotifierItems,
        // so a cached read would return the list from proxy creation.
        let watcher = StatusNotifierWatcherProxy::builder(conn)
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;

        // The watcher may be ours or another process's, and can change hands
        // (it exits, or another bar replaces it). Subscribed first so a change
        // during setup is not missed.
        let mut owner_changed = watcher.inner().receive_owner_changed().await?;

        if let Some(host) = host
            && let Err(err) = watcher.register_status_notifier_host(host).await
        {
            debug!("Failed to register tray host {host}: {err}");
        }

        // Subscribe before reading the list: an item registering in between is
        // then seen by at least one of the two, and duplicates are skipped.
        let mut registered = watcher.receive_status_notifier_item_registered().await?;
        let mut unregistered = watcher.receive_status_notifier_item_unregistered().await?;
        let names = watcher.registered_status_notifier_items().await?;

        let mut streams = ItemStreams::default();
        let mut retries = RetryQueue::new();
        let mut items = Vec::with_capacity(names.len());

        for name in names {
            if streams.contains(&name) {
                continue;
            }
            if let Some(item) = Self::load_item(conn, name, 0, &mut streams, &mut retries).await {
                items.push(item);
            }
        }

        info!("Tray service initialized with {} items", items.len());

        let _ = output
            .send(ServiceEvent::Init(TrayService {
                data: TrayData(items),
                _conn: conn.clone(),
            }))
            .await;
        *failures = 0;

        info!("Listening for tray events");

        loop {
            let event = tokio::select! {
                Some(signal) = registered.next() => {
                    let Ok(args) = signal.args() else {
                        continue;
                    };
                    let name = args.service.to_string();
                    debug!("registered {name}");

                    if streams.contains(&name) {
                        continue;
                    }
                    Self::load_item(conn, name, 0, &mut streams, &mut retries)
                        .await
                        .map(TrayEvent::Registered)
                }
                Some((name, attempt)) = retries.next() => {
                    if streams.contains(&name) {
                        continue;
                    }
                    Self::load_item(conn, name, attempt, &mut streams, &mut retries)
                        .await
                        .map(TrayEvent::Registered)
                }
                Some(signal) = unregistered.next() => {
                    let Ok(args) = signal.args() else {
                        continue;
                    };
                    let name = args.service.to_string();
                    debug!("unregistered {name}");

                    streams.remove(&name);
                    Some(TrayEvent::Unregistered(name))
                }
                Some(event) = streams.events.next() => Some(event),
                Some(owner) = owner_changed.next() => {
                    info!("Tray watcher owner changed to {owner:?}, reloading items");
                    if owner.is_none() {
                        let _ = output
                            .send(ServiceEvent::Init(TrayService {
                                data: TrayData::default(),
                                _conn: conn.clone(),
                            }))
                            .await;
                    }
                    return Ok(());
                }
                else => break,
            };

            if let Some(event) = event {
                debug!("tray data {event:?}");
                let _ = output.send(ServiceEvent::Update(event)).await;
            }
        }

        Err(anyhow::anyhow!("watcher signal streams ended"))
    }

    async fn start_listening(state: State, output: &mut Sender<ServiceEvent<Self>>) -> State {
        match state {
            State::Init => match StatusNotifierWatcher::start_server().await {
                Ok(conn) => {
                    let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
                    let host = match conn
                        .request_name_with_flags(host.as_str(), RequestNameFlags::DoNotQueue.into())
                        .await
                    {
                        Ok(_) => Some(host),
                        Err(err) => {
                            warn!("Failed to own {host}: {err}");
                            None
                        }
                    };

                    State::Active {
                        conn,
                        host,
                        failures: 0,
                    }
                }
                Err(err) => {
                    error!("Failed to connect to system bus: {err}");

                    State::Error
                }
            },
            State::Active {
                conn,
                host,
                mut failures,
            } => {
                if let Err(err) =
                    TrayService::run_host(&conn, host.as_deref(), output, &mut failures).await
                {
                    let backoff = Duration::from_secs(1 << failures.min(5)).min(MAX_HOST_BACKOFF);
                    error!("Tray host failed: {err}, retrying in {backoff:?}");

                    tokio::time::sleep(backoff).await;
                    failures += 1;
                }

                State::Active {
                    conn,
                    host,
                    failures,
                }
            }
            State::Error => {
                error!("Tray service error");

                let _ = pending::<u8>().next().await;
                State::Error
            }
        }
    }

    async fn menu_voice_selected(
        menu_proxy: &DBusMenuProxy<'_>,
        id: i32,
    ) -> anyhow::Result<Layout> {
        let value = zbus::zvariant::Value::I32(32).try_to_owned()?;
        menu_proxy
            .event(
                id,
                "clicked",
                &value,
                chrono::offset::Local::now().timestamp_subsec_micros(),
            )
            .await?;

        let (_, layout) = menu_proxy.get_layout(0, -1, &[]).await?;

        Ok(layout)
    }
}

impl ReadOnlyService for TrayService {
    type UpdateEvent = TrayEvent;
    type Error = ();

    fn update(&mut self, event: Self::UpdateEvent) {
        match event {
            TrayEvent::Registered(new_item) => {
                match self
                    .data
                    .0
                    .iter_mut()
                    .find(|item| item.name == new_item.name)
                {
                    Some(existing_item) => {
                        *existing_item = new_item;
                    }
                    _ => {
                        self.data.0.push(new_item);
                    }
                }
            }
            TrayEvent::IconChanged(name, icon) => {
                if let Some(item) = self.data.0.iter_mut().find(|item| item.name == name) {
                    item.icon = icon;
                }
            }
            TrayEvent::MenuLayoutChanged(name, layout) => {
                if let Some(item) = self.data.0.iter_mut().find(|item| item.name == name) {
                    debug!("menu layout updated, {layout:?}");
                    item.menu = Some(layout);
                }
            }
            TrayEvent::Unregistered(name) => {
                self.data.0.retain(|item| item.name != name);
            }
            TrayEvent::None => {}
        }
    }

    fn subscribe() -> iced::Subscription<ServiceEvent<Self>> {
        Subscription::run_with(TypeId::of::<Self>(), |_| {
            channel(100, async |mut output| {
                let mut state = State::Init;

                loop {
                    state = TrayService::start_listening(state, &mut output).await;
                }
            })
        })
    }
}

#[derive(Debug, Clone)]
pub enum TrayCommand {
    MenuSelected(String, i32),
    Activate(String),
}

impl Service for TrayService {
    type Command = TrayCommand;

    fn command(&mut self, command: Self::Command) -> Task<ServiceEvent<Self>> {
        match command {
            TrayCommand::MenuSelected(name, id) => {
                let menu_proxy = self
                    .data
                    .iter()
                    .find(|item| item.name == name)
                    .and_then(|item| item.menu_proxy.clone());
                if let Some(proxy) = menu_proxy {
                    let name_cb = name.clone();
                    Task::perform(
                        async move {
                            debug!("Click tray menu voice {name} : {id}");
                            TrayService::menu_voice_selected(&proxy, id).await
                        },
                        move |new_layout| match new_layout {
                            Ok(new_layout) => ServiceEvent::Update(TrayEvent::MenuLayoutChanged(
                                name_cb.clone(),
                                new_layout,
                            )),
                            _ => ServiceEvent::Update(TrayEvent::None),
                        },
                    )
                } else {
                    Task::none()
                }
            }
            TrayCommand::Activate(name) => {
                let item = self.data.iter().find(|item| item.name == name);
                if let Some(item) = item {
                    Task::perform(
                        {
                            let proxy = item.item_proxy.clone();
                            async move {
                                debug!("Activate tray item {name}");
                                let _ = proxy.activate(0, 0).await;
                            }
                        },
                        |_| ServiceEvent::Update(TrayEvent::None),
                    )
                } else {
                    Task::none()
                }
            }
        }
    }
}
