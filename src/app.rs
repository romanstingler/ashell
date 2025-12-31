use crate::{
    HEIGHT, centerbox,
    config::{self, AppearanceStyle, BarConfig, Config, ModuleDef, Modules, Position},
    get_log_spec,
    menu::{MenuSize, MenuType},
    modules::{
        self,
        app_launcher::{self, AppLauncher},
        clipboard::{self, Clipboard},
        clock::Clock,
        custom_module::{self, Custom},
        keyboard_layout::KeyboardLayout,
        keyboard_submap::KeyboardSubmap,
        media_player::MediaPlayer,
        privacy::Privacy,
        settings::Settings,
        system_info::SystemInfo,
        tray::TrayModule,
        updates::Updates,
        window_title::WindowTitle,
        workspaces::Workspaces,
    },
    outputs::{HasOutput, Outputs},
    position_button::ButtonUIRef,
    theme::{AshellTheme, backdrop_color, darken_color},
};
use flexi_logger::LoggerHandle;
use iced::{
    Alignment, Color, Element, Gradient, Length, Radians, Subscription, Task, Theme,
    daemon::Appearance,
    event::{
        listen_with,
        wayland::{Event as WaylandEvent, OutputEvent},
    },
    gradient::Linear,
    keyboard,
    widget::{Row, container, mouse_area},
    window::Id,
};
use log::{debug, info, warn};
use std::{collections::HashMap, f32::consts::PI, path::PathBuf};
use wayland_client::protocol::wl_output::WlOutput;

pub struct GeneralConfig {
    pub config: Config,
    pub modules: Modules,
    pub enable_esc_key: bool,
}

pub struct App {
    config_path: PathBuf,
    pub theme: AshellTheme,
    logger: LoggerHandle,
    pub general_config: GeneralConfig,
    pub outputs: Outputs,
    pub app_launcher: Option<AppLauncher>,
    pub custom: HashMap<String, Custom>,
    pub updates: Option<Updates>,
    pub clipboard: Option<Clipboard>,
    pub workspaces: Workspaces,
    pub window_title: WindowTitle,
    pub system_info: SystemInfo,
    pub keyboard_layout: KeyboardLayout,
    pub keyboard_submap: KeyboardSubmap,
    pub tray: TrayModule,
    pub clock: Clock,
    pub privacy: Privacy,
    pub settings: Settings,
    pub media_player: MediaPlayer,
}

#[derive(Debug, Clone)]
pub enum Message {
    None,
    ConfigChanged(Box<Config>),
    ToggleMenu(MenuType, Id, ButtonUIRef),
    CloseMenu(Id),
    Clipboard(clipboard::Message),
    AppLauncher(app_launcher::Message),
    Custom(String, custom_module::Message),
    Updates(modules::updates::Message),
    Workspaces(modules::workspaces::Message),
    WindowTitle(modules::window_title::Message),
    SystemInfo(modules::system_info::Message),
    KeyboardLayout(modules::keyboard_layout::Message),
    KeyboardSubmap(modules::keyboard_submap::Message),
    Tray(modules::tray::Message),
    Clock(modules::clock::Message),
    Privacy(modules::privacy::Message),
    Settings(modules::settings::Message),
    MediaPlayer(modules::media_player::Message),
    OutputEvent((OutputEvent, WlOutput)),
    CloseAllMenus,
}

impl App {
    pub fn new(
        (logger, config, config_path): (LoggerHandle, Config, PathBuf),
    ) -> impl FnOnce() -> (Self, Task<Message>) {
        move || {
            let (outputs, task) =
                Outputs::new(config.get_bar_configs(), config.appearance.scale_factor);

            let custom = config
                .custom_modules
                .clone()
                .into_iter()
                .map(|o| (o.name.clone(), Custom::new(o)))
                .collect();

            (
                App {
                    config_path,
                    theme: AshellTheme::new(config.position, &config.appearance),
                    logger,
                    general_config: GeneralConfig {
                        modules: config.modules.clone(),
                        enable_esc_key: config.enable_esc_key,
                        config: config.clone(),
                    },
                    outputs,
                    app_launcher: config.app_launcher_cmd.map(AppLauncher::new),
                    custom,
                    updates: config.updates.map(Updates::new),
                    clipboard: config.clipboard_cmd.map(Clipboard::new),
                    workspaces: Workspaces::new(config.workspaces),
                    window_title: WindowTitle::new(config.window_title),
                    system_info: SystemInfo::new(config.system_info),
                    keyboard_layout: KeyboardLayout::new(config.keyboard_layout),
                    keyboard_submap: KeyboardSubmap::default(),
                    tray: TrayModule::default(),
                    clock: Clock::new(config.clock),
                    privacy: Privacy::default(),
                    settings: Settings::new(config.settings),
                    media_player: MediaPlayer::new(config.media_player),
                },
                task,
            )
        }
    }

    fn refesh_config(&mut self, config: Box<Config>) {
        self.general_config = GeneralConfig {
            modules: config.modules.clone(),
            enable_esc_key: config.enable_esc_key,
            config: *config.clone(),
        };
        self.theme = AshellTheme::new(config.position, &config.appearance);
        let custom = config
            .custom_modules
            .into_iter()
            .map(|o| (o.name.clone(), Custom::new(o)))
            .collect();

        self.app_launcher = config.app_launcher_cmd.map(AppLauncher::new);
        self.custom = custom;
        self.updates = config.updates.map(Updates::new);
        self.clipboard = config.clipboard_cmd.map(Clipboard::new);

        // ignore task, since config change should not generate any
        let _ = self
            .workspaces
            .update(modules::workspaces::Message::ConfigReloaded(
                config.workspaces,
            ))
            .map(Message::Workspaces);

        self.window_title
            .update(modules::window_title::Message::ConfigReloaded(
                config.window_title,
            ));

        self.system_info = SystemInfo::new(config.system_info);

        let _ = self
            .keyboard_layout
            .update(modules::keyboard_layout::Message::ConfigReloaded(
                config.keyboard_layout,
            ))
            .map(Message::KeyboardLayout);

        self.keyboard_submap = KeyboardSubmap::default();
        self.clock = Clock::new(config.clock);
        self.settings
            .update(modules::settings::Message::ConfigReloaded(config.settings));
        self.media_player
            .update(modules::media_player::Message::ConfigReloaded(
                config.media_player,
            ));
    }

    pub fn title(&self, _id: Id) -> String {
        String::from("ashell")
    }

    pub fn theme(&self, _id: Id) -> Theme {
        self.theme.get_theme().clone()
    }

    pub fn style(&self, theme: &Theme) -> Appearance {
        Appearance {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
            icon_color: theme.palette().text,
        }
    }

    pub fn scale_factor(&self, _id: Id) -> f64 {
        self.theme.scale_factor
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::None => Task::none(),
            Message::ConfigChanged(config) => {
                info!("New config: {config:?}");
                let mut tasks = Vec::new();
                info!(
                    "Current outputs: {:?}, new outputs: {:?}",
                    self.general_config.config.outputs, config.outputs
                );
                if self.general_config.config.outputs != config.outputs
                    || self.general_config.config.get_bar_configs() != config.get_bar_configs()
                    || self.theme.bar_position != config.position
                    || self.theme.bar_style != config.appearance.style
                    || self.theme.scale_factor != config.appearance.scale_factor
                {
                    warn!("Outputs changed, syncing");
                    tasks.push(self.outputs.sync(
                        config.get_bar_configs(),
                        &config.outputs,
                        config.appearance.scale_factor,
                    ));
                }

                self.logger.set_new_spec(get_log_spec(&config.log_level));
                self.refesh_config(config);

                Task::batch(tasks)
            }
            Message::ToggleMenu(menu_type, id, button_ui_ref) => {
                let mut cmd = vec![];
                match &menu_type {
                    MenuType::Updates => {
                        if let Some(updates) = self.updates.as_mut() {
                            updates.update(modules::updates::Message::MenuOpened);
                        }
                    }
                    MenuType::Tray(name) => {
                        self.tray
                            .update(modules::tray::Message::MenuOpened(name.clone()));
                    }
                    MenuType::Settings => {
                        cmd.push(
                            match self.settings.update(modules::settings::Message::MenuOpened) {
                                modules::settings::Action::Command(task) => {
                                    task.map(Message::Settings)
                                }
                                _ => Task::none(),
                            },
                        );
                    }
                    _ => {}
                };
                cmd.push(self.outputs.toggle_menu(
                    id,
                    menu_type,
                    button_ui_ref,
                    self.general_config.enable_esc_key,
                ));

                Task::batch(cmd)
            }
            Message::CloseMenu(id) => self
                .outputs
                .close_menu(id, self.general_config.enable_esc_key),
            Message::AppLauncher(msg) => {
                if let Some(app_launcher) = self.app_launcher.as_mut() {
                    app_launcher.update(msg);
                }

                Task::none()
            }
            Message::Custom(name, msg) => {
                if let Some(custom) = self.custom.get_mut(&name) {
                    custom.update(msg);
                }

                Task::none()
            }
            Message::Updates(msg) => {
                if let Some(updates) = self.updates.as_mut() {
                    match updates.update(msg) {
                        modules::updates::Action::None => Task::none(),
                        modules::updates::Action::CheckForUpdates(task) => {
                            task.map(Message::Updates)
                        }
                        modules::updates::Action::CloseMenu(id, task) => Task::batch(vec![
                            task.map(Message::Updates),
                            self.outputs.close_menu_if(
                                id,
                                MenuType::Updates,
                                self.general_config.enable_esc_key,
                            ),
                        ]),
                    }
                } else {
                    Task::none()
                }
            }
            Message::Clipboard(msg) => {
                if let Some(clipboard) = self.clipboard.as_mut() {
                    clipboard.update(msg);
                }

                Task::none()
            }
            Message::Workspaces(msg) => self.workspaces.update(msg).map(Message::Workspaces),
            Message::WindowTitle(msg) => {
                self.window_title.update(msg);
                Task::none()
            }
            Message::SystemInfo(msg) => {
                self.system_info.update(msg);
                Task::none()
            }
            Message::KeyboardLayout(message) => self
                .keyboard_layout
                .update(message)
                .map(Message::KeyboardLayout),
            Message::KeyboardSubmap(message) => {
                self.keyboard_submap.update(message);
                Task::none()
            }
            Message::Tray(msg) => match self.tray.update(msg) {
                modules::tray::Action::None => Task::none(),
                modules::tray::Action::ToggleMenu(name, id, button_ui_ref) => {
                    self.outputs.toggle_menu(
                        id,
                        MenuType::Tray(name),
                        button_ui_ref,
                        self.general_config.enable_esc_key,
                    )
                }
                modules::tray::Action::TrayMenuCommand(task) => Task::batch(vec![
                    self.outputs
                        .close_all_menus(self.general_config.enable_esc_key),
                    task.map(Message::Tray),
                ]),
                modules::tray::Action::CloseTrayMenu(name) => self
                    .outputs
                    .close_all_menu_if(MenuType::Tray(name), self.general_config.enable_esc_key),
            },
            Message::Clock(message) => {
                self.clock.update(message);
                Task::none()
            }
            Message::Privacy(msg) => {
                self.privacy.update(msg);
                Task::none()
            }
            Message::Settings(message) => match self.settings.update(message) {
                modules::settings::Action::None => Task::none(),
                modules::settings::Action::Command(task) => task.map(Message::Settings),
                modules::settings::Action::CloseMenu(id) => self
                    .outputs
                    .close_menu(id, self.general_config.enable_esc_key),
                modules::settings::Action::RequestKeyboard(id) => self.outputs.request_keyboard(id),
                modules::settings::Action::ReleaseKeyboard(id) => self.outputs.release_keyboard(id),
                modules::settings::Action::ReleaseKeyboardWithCommand(id, task) => {
                    Task::batch(vec![
                        task.map(Message::Settings),
                        self.outputs.release_keyboard(id),
                    ])
                }
            },
            Message::OutputEvent((event, wl_output)) => match event {
                iced::event::wayland::OutputEvent::Created(info) => {
                    info!("Output created: {info:?}");
                    let name = info
                        .as_ref()
                        .and_then(|info| info.description.as_deref())
                        .unwrap_or("");

                    self.outputs.add(
                        self.general_config.config.get_bar_configs(),
                        &self.general_config.config.outputs,
                        name,
                        wl_output,
                        self.theme.scale_factor,
                    )
                }
                iced::event::wayland::OutputEvent::Removed => {
                    info!("Output destroyed");
                    self.outputs.remove(
                        self.general_config.config.get_bar_configs(),
                        wl_output,
                        self.theme.scale_factor,
                    )
                }
                _ => Task::none(),
            },
            Message::MediaPlayer(msg) => match self.media_player.update(msg) {
                modules::media_player::Action::None => Task::none(),
                modules::media_player::Action::Command(task) => task.map(Message::MediaPlayer),
            },
            Message::CloseAllMenus => {
                if self.outputs.menu_is_open() {
                    self.outputs
                        .close_all_menus(self.general_config.enable_esc_key)
                } else {
                    Task::none()
                }
            }
        }
    }

    pub fn view(&'_ self, id: Id) -> Element<'_, Message> {
        match self.outputs.has(id) {
            Some(HasOutput::Main(shell_info)) => {
                let [left, center, right] =
                    self.modules_section_for_bar(id, &self.theme, &shell_info.config);
                let style = shell_info
                    .config
                    .appearance
                    .as_ref()
                    .map(|a| a.style)
                    .unwrap_or(self.theme.bar_style);

                let centerbox = centerbox::Centerbox::new([left, center, right])
                    .spacing(self.theme.space.xxs)
                    .width(Length::Fill)
                    .align_items(Alignment::Center)
                    .height(if style == AppearanceStyle::Islands {
                        HEIGHT
                    } else {
                        HEIGHT - 8.
                    } as f32)
                    .padding(if style == AppearanceStyle::Islands {
                        [self.theme.space.xxs, self.theme.space.xxs]
                    } else {
                        [0, 0]
                    });

                let status_bar = container(centerbox).style(move |t: &Theme| container::Style {
                    background: match style {
                        AppearanceStyle::Gradient => Some({
                            let start_color =
                                t.palette().background.scale_alpha(self.theme.opacity);

                            let start_color = if self.outputs.menu_is_open() {
                                darken_color(start_color, self.theme.menu.backdrop)
                            } else {
                                start_color
                            };

                            let end_color = if self.outputs.menu_is_open() {
                                backdrop_color(self.theme.menu.backdrop)
                            } else {
                                Color::TRANSPARENT
                            };

                            Gradient::Linear(
                                Linear::new(Radians(PI))
                                    .add_stop(
                                        0.0,
                                        match shell_info.config.position {
                                            Position::Top => start_color,
                                            Position::Bottom => end_color,
                                        },
                                    )
                                    .add_stop(
                                        1.0,
                                        match shell_info.config.position {
                                            Position::Top => end_color,
                                            Position::Bottom => start_color,
                                        },
                                    ),
                            )
                            .into()
                        }),
                        AppearanceStyle::Solid => Some({
                            let bg = t.palette().background.scale_alpha(self.theme.opacity);
                            if self.outputs.menu_is_open() {
                                darken_color(bg, self.theme.menu.backdrop)
                            } else {
                                bg
                            }
                            .into()
                        }),
                        AppearanceStyle::Islands => {
                            if self.outputs.menu_is_open() {
                                Some(backdrop_color(self.theme.menu.backdrop).into())
                            } else {
                                None
                            }
                        }
                    },
                    ..Default::default()
                });

                if self.outputs.menu_is_open() {
                    mouse_area(status_bar)
                        .on_release(Message::CloseMenu(id))
                        .into()
                } else {
                    status_bar.into()
                }
            }
            Some(HasOutput::Menu(menu_info)) => match menu_info {
                Some((MenuType::Updates, button_ui_ref)) => {
                    if let Some(updates) = self.updates.as_ref() {
                        let position = self.get_bar_position(id).unwrap_or(self.theme.bar_position);
                        self.menu_wrapper(
                            id,
                            updates.menu_view(id, &self.theme).map(Message::Updates),
                            MenuSize::Small,
                            *button_ui_ref,
                            position,
                        )
                    } else {
                        Row::new().into()
                    }
                }
                Some((MenuType::Tray(name), button_ui_ref)) => {
                    let position = self.get_bar_position(id).unwrap_or(self.theme.bar_position);
                    self.menu_wrapper(
                        id,
                        self.tray.menu_view(&self.theme, name).map(Message::Tray),
                        MenuSize::Medium,
                        *button_ui_ref,
                        position,
                    )
                }
                Some((MenuType::Settings, button_ui_ref)) => {
                    let position = self.get_bar_position(id).unwrap_or(self.theme.bar_position);
                    self.menu_wrapper(
                        id,
                        self.settings
                            .menu_view(id, &self.theme, position)
                            .map(Message::Settings),
                        MenuSize::Medium,
                        *button_ui_ref,
                        position,
                    )
                }
                Some((MenuType::MediaPlayer, button_ui_ref)) => {
                    let position = self.get_bar_position(id).unwrap_or(self.theme.bar_position);
                    self.menu_wrapper(
                        id,
                        self.media_player
                            .menu_view(&self.theme)
                            .map(Message::MediaPlayer),
                        MenuSize::Large,
                        *button_ui_ref,
                        position,
                    )
                }
                Some((MenuType::SystemInfo, button_ui_ref)) => {
                    let position = self.get_bar_position(id).unwrap_or(self.theme.bar_position);
                    self.menu_wrapper(
                        id,
                        self.system_info
                            .menu_view(&self.theme)
                            .map(Message::SystemInfo),
                        MenuSize::Medium,
                        *button_ui_ref,
                        position,
                    )
                }
                None => Row::new().into(),
            },
            None => Row::new().into(),
        }
    }

    fn modules_section_for_bar<'a>(
        &'a self,
        id: Id,
        theme: &'a AshellTheme,
        bar_config: &'a BarConfig,
    ) -> [Element<'a, Message>; 3] {
        let bar_modules = bar_config
            .modules
            .as_ref()
            .unwrap_or(&self.general_config.modules);
        [
            self.modules_view(&bar_modules.left, id, theme),
            self.modules_view(&bar_modules.center, id, theme),
            self.modules_view(&bar_modules.right, id, theme),
        ]
    }

    pub fn get_bar_position(&self, id: Id) -> Option<Position> {
        self.outputs.get_bar_config(id).map(|c| c.position)
    }

    fn modules_view<'a>(
        &'a self,
        modules: &'a [ModuleDef],
        id: Id,
        theme: &'a AshellTheme,
    ) -> Element<'a, Message> {
        let mut row = Row::new()
            .spacing(theme.space.xxs)
            .align_y(Alignment::Center);

        for module_def in modules {
            row = row.push_maybe(match module_def {
                ModuleDef::Single(module) => self.single_module_wrapper(id, theme, module),
                ModuleDef::Group(group) => self.group_module_wrapper(id, theme, group),
            });
        }

        row.into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            config::subscription(&self.config_path),
            listen_with(move |evt, _, _| match evt {
                iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(
                    WaylandEvent::Output(event, wl_output),
                )) => {
                    debug!("Wayland event: {event:?}");
                    Some(Message::OutputEvent((event, wl_output)))
                }
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                    debug!("Keyboard event received: {key:?}");
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        debug!("ESC key pressed, closing all menus");
                        Some(Message::CloseAllMenus)
                    } else {
                        None
                    }
                }
                _ => None,
            }),
        ];

        for bar_config in self.general_config.config.get_bar_configs() {
            let modules = bar_config
                .modules
                .as_ref()
                .unwrap_or(&self.general_config.modules);
            subscriptions.push(Subscription::batch(
                self.modules_subscriptions(&modules.left),
            ));
            subscriptions.push(Subscription::batch(
                self.modules_subscriptions(&modules.center),
            ));
            subscriptions.push(Subscription::batch(
                self.modules_subscriptions(&modules.right),
            ));
        }

        Subscription::batch(subscriptions)
    }
}
