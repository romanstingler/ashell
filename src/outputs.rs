use iced::{
    Task,
    platform_specific::shell::commands::layer_surface::{
        Anchor, KeyboardInteractivity, Layer, destroy_layer_surface, get_layer_surface,
        set_exclusive_zone, set_keyboard_interactivity, set_size,
    },
    runtime::platform_specific::wayland::layer_surface::{IcedOutput, SctkLayerSurfaceSettings},
    window::Id,
};
use log::debug;
use wayland_client::protocol::wl_output::WlOutput;

use crate::{
    HEIGHT,
    config::{self, AppearanceStyle, BarConfig, Position},
    menu::{Menu, MenuType},
    position_button::ButtonUIRef,
};

#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub id: Id,
    pub config: BarConfig,
    pub menu: Menu,
    pub scale_factor: f64,
}

#[derive(Debug, Clone)]
pub struct Outputs(Vec<(String, Vec<ShellInfo>, Option<WlOutput>)>);

pub enum HasOutput<'a> {
    Main(&'a ShellInfo),
    Menu(Option<&'a (MenuType, ButtonUIRef)>),
}

impl Outputs {
    pub fn new<Message: 'static>(
        bar_configs: Vec<BarConfig>,
        scale_factor: f64,
    ) -> (Self, Task<Message>) {
        let (infos, task) = Self::create_output_layers(None, bar_configs, scale_factor);

        (Self(vec![("Fallback".to_string(), infos, None)]), task)
    }

    fn get_height(style: AppearanceStyle, scale_factor: f64) -> f64 {
        (HEIGHT
            - match style {
                AppearanceStyle::Solid | AppearanceStyle::Gradient => 8.,
                AppearanceStyle::Islands => 0.,
            })
            * scale_factor
    }

    fn create_output_layers<Message: 'static>(
        wl_output: Option<WlOutput>,
        bar_configs: Vec<BarConfig>,
        scale_factor: f64,
    ) -> (Vec<ShellInfo>, Task<Message>) {
        let mut infos = Vec::new();
        let mut tasks = Vec::new();

        for config in bar_configs {
            let id = Id::unique();
            let style = config
                .appearance
                .as_ref()
                .map(|a| a.style)
                .unwrap_or(AppearanceStyle::default());
            let height = Self::get_height(style, scale_factor);

            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id,
                namespace: "ashell-main-layer".to_string(),
                size: Some((None, Some(height as u32))),
                layer: Layer::Bottom,
                keyboard_interactivity: KeyboardInteractivity::None,
                exclusive_zone: height as i32,
                output: wl_output.clone().map_or(IcedOutput::Active, |wl_output| {
                    IcedOutput::Output(wl_output)
                }),
                anchor: match config.position {
                    Position::Top => Anchor::TOP,
                    Position::Bottom => Anchor::BOTTOM,
                } | Anchor::LEFT
                    | Anchor::RIGHT,
                ..Default::default()
            }));

            let menu_id = Id::unique();
            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id: menu_id,
                namespace: "ashell-main-layer".to_string(),
                size: Some((None, None)),
                layer: Layer::Background,
                keyboard_interactivity: KeyboardInteractivity::None,
                output: wl_output.clone().map_or(IcedOutput::Active, |wl_output| {
                    IcedOutput::Output(wl_output)
                }),
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                ..Default::default()
            }));

            infos.push(ShellInfo {
                id,
                menu: Menu::new(menu_id),
                config,
                scale_factor,
            });
        }

        (infos, Task::batch(tasks))
    }

    fn name_in_config(name: &str, outputs: &config::Outputs) -> bool {
        match outputs {
            config::Outputs::All => true,
            config::Outputs::Active => false,
            config::Outputs::Targets(request_outputs) => {
                request_outputs.iter().any(|output| name.contains(output))
            }
        }
    }

    pub fn has(&'_ self, id: Id) -> Option<HasOutput<'_>> {
        self.0.iter().find_map(|(_, infos, _)| {
            infos.iter().find_map(|info| {
                if info.id == id {
                    Some(HasOutput::Main(info))
                } else if info.menu.id == id {
                    Some(HasOutput::Menu(info.menu.menu_info.as_ref()))
                } else {
                    None
                }
            })
        })
    }

    pub fn get_monitor_name(&self, id: Id) -> Option<&str> {
        self.0.iter().find_map(|(name, infos, _)| {
            infos.iter().find_map(|info| {
                if info.id == id {
                    Some(name.as_str())
                } else {
                    None
                }
            })
        })
    }

    pub fn has_name(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|(n, infos, _)| !infos.is_empty() && n.as_str().contains(name))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add<Message: 'static>(
        &mut self,
        bar_configs: Vec<BarConfig>,
        request_outputs: &config::Outputs,
        name: &str,
        wl_output: WlOutput,
        scale_factor: f64,
    ) -> Task<Message> {
        let target = Self::name_in_config(name, request_outputs);

        if target {
            debug!("Found target output, creating a new layer surface");

            let (infos, task) =
                Self::create_output_layers(Some(wl_output.clone()), bar_configs, scale_factor);

            let destroy_task = match self.0.iter().position(|(key, _, _)| key.as_str() == name) {
                Some(index) => {
                    let old_output = self.0.swap_remove(index);
                    let mut destroy_tasks = Vec::new();

                    for shell_info in old_output.1 {
                        destroy_tasks.push(destroy_layer_surface(shell_info.id));
                        destroy_tasks.push(destroy_layer_surface(shell_info.menu.id));
                    }
                    Task::batch(destroy_tasks)
                }
                _ => Task::none(),
            };

            self.0.push((name.to_owned(), infos, Some(wl_output)));

            // remove fallback layer surface
            let destroy_fallback_task =
                match self.0.iter().position(|(_, _, output)| output.is_none()) {
                    Some(index) => {
                        let old_output = self.0.swap_remove(index);
                        let mut destroy_tasks = Vec::new();

                        for shell_info in old_output.1 {
                            destroy_tasks.push(destroy_layer_surface(shell_info.id));
                            destroy_tasks.push(destroy_layer_surface(shell_info.menu.id));
                        }
                        Task::batch(destroy_tasks)
                    }
                    _ => Task::none(),
                };

            Task::batch(vec![destroy_task, destroy_fallback_task, task])
        } else {
            self.0.push((name.to_owned(), Vec::new(), Some(wl_output)));

            Task::none()
        }
    }

    pub fn remove<Message: 'static>(
        &mut self,
        bar_configs: Vec<BarConfig>,
        wl_output: WlOutput,
        scale_factor: f64,
    ) -> Task<Message> {
        match self.0.iter().position(|(_, _, assigned_wl_output)| {
            assigned_wl_output
                .as_ref()
                .is_some_and(|assigned_wl_output| *assigned_wl_output == wl_output)
        }) {
            Some(index_to_remove) => {
                debug!("Removing layer surface for output");

                let (name, infos, wl_output) = self.0.swap_remove(index_to_remove);

                let mut destroy_tasks = Vec::new();
                for shell_info in infos {
                    destroy_tasks.push(destroy_layer_surface(shell_info.id));
                    destroy_tasks.push(destroy_layer_surface(shell_info.menu.id));
                }
                let destroy_task = Task::batch(destroy_tasks);

                self.0.push((name, Vec::new(), wl_output));

                if self.0.iter().any(|(_, infos, _)| !infos.is_empty()) {
                    Task::batch(vec![destroy_task])
                } else {
                    debug!("No outputs left, creating a fallback layer surface");

                    let (infos, task) = Self::create_output_layers(None, bar_configs, scale_factor);

                    self.0.push(("Fallback".to_string(), infos, None));

                    Task::batch(vec![destroy_task, task])
                }
            }
            _ => Task::none(),
        }
    }

    pub fn sync<Message: 'static>(
        &mut self,
        bar_configs: Vec<BarConfig>,
        request_outputs: &config::Outputs,
        scale_factor: f64,
    ) -> Task<Message> {
        debug!("Syncing outputs: {self:?}, request_outputs: {request_outputs:?}");

        let to_remove = self
            .0
            .iter()
            .filter_map(|(name, infos, wl_output)| {
                if !Self::name_in_config(name, request_outputs) && !infos.is_empty() {
                    Some(wl_output.clone())
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>();
        debug!("Removing outputs: {to_remove:?}");

        let to_add = self
            .0
            .iter()
            .filter_map(|(name, infos, wl_output)| {
                if Self::name_in_config(name, request_outputs) && infos.is_empty() {
                    Some((name.clone(), wl_output.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        debug!("Adding outputs: {to_add:?}");

        let mut tasks = Vec::new();

        for (name, wl_output) in to_add {
            if let Some(wl_output) = wl_output {
                tasks.push(self.add(
                    bar_configs.clone(),
                    request_outputs,
                    name.as_str(),
                    wl_output,
                    scale_factor,
                ));
            }
        }

        for wl_output in to_remove {
            tasks.push(self.remove(bar_configs.clone(), wl_output, scale_factor));
        }

        // Handle style or scale_factor changes for existing bars
        for (_, infos, _) in self.0.iter_mut() {
            // If the number of bars changed, we might need a full recreate, but for now let's sync existing ones
            // This is a simple implementation that might need more complexity for dynamic bar addition/removal
            for (i, shell_info) in infos.iter_mut().enumerate() {
                if let Some(config) = bar_configs.get(i) {
                    let style = config
                        .appearance
                        .as_ref()
                        .map(|a| a.style)
                        .unwrap_or(AppearanceStyle::default());
                    if shell_info.config != *config || shell_info.scale_factor != scale_factor {
                        debug!(
                            "Change bar config or scale_factor for output: {:?}, new style {:?}, new scale_factor {:?}",
                            shell_info.id, style, scale_factor
                        );
                        shell_info.config = config.clone();
                        shell_info.scale_factor = scale_factor;
                        let height = Self::get_height(style, scale_factor);
                        tasks.push(Task::batch(vec![
                            set_size(shell_info.id, None, Some(height as u32)),
                            set_exclusive_zone(shell_info.id, height as i32),
                        ]));
                    }
                }
            }
        }

        Task::batch(tasks)
    }

    pub fn get_bar_config(&self, id: Id) -> Option<BarConfig> {
        for (_, infos, _) in self.0.iter() {
            for shell_info in infos {
                if shell_info.id == id || shell_info.menu.id == id {
                    return Some(shell_info.config.clone());
                }
            }
        }
        None
    }

    pub fn menu_is_open(&self) -> bool {
        self.0.iter().any(|(_, infos, _)| {
            infos
                .iter()
                .any(|shell_info| shell_info.menu.menu_info.is_some())
        })
    }

    pub fn toggle_menu<Message: 'static>(
        &mut self,
        id: Id,
        menu_type: MenuType,
        button_ui_ref: ButtonUIRef,
        request_keyboard: bool,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut target_id = None;

        for (_, infos, _) in self.0.iter_mut() {
            for shell_info in infos.iter_mut() {
                if shell_info.id == id || shell_info.menu.id == id {
                    target_id = Some(shell_info.id);
                    tasks.push(shell_info.menu.toggle(
                        menu_type.clone(),
                        button_ui_ref,
                        request_keyboard,
                    ));
                } else {
                    tasks.push(shell_info.menu.close());
                }
            }
        }

        let task = Task::batch(tasks);

        if request_keyboard {
            if let Some(id) = target_id {
                if self.menu_is_open() {
                    Task::batch(vec![
                        task,
                        set_keyboard_interactivity(id, KeyboardInteractivity::OnDemand),
                    ])
                } else {
                    Task::batch(vec![
                        task,
                        set_keyboard_interactivity(id, KeyboardInteractivity::None),
                    ])
                }
            } else {
                task
            }
        } else {
            task
        }
    }

    pub fn close_menu<Message: 'static>(
        &mut self,
        id: Id,
        esc_button_enabled: bool,
    ) -> Task<Message> {
        let mut task = Task::none();
        for (_, infos, _) in self.0.iter_mut() {
            for shell_info in infos.iter_mut() {
                if shell_info.id == id || shell_info.menu.id == id {
                    task = shell_info.menu.close();
                }
            }
        }

        if esc_button_enabled && !self.menu_is_open() {
            Task::batch(vec![
                task,
                set_keyboard_interactivity(id, KeyboardInteractivity::None),
            ])
        } else {
            task
        }
    }

    pub fn close_menu_if<Message: 'static>(
        &mut self,
        id: Id,
        menu_type: MenuType,
        esc_button_enabled: bool,
    ) -> Task<Message> {
        let mut task = Task::none();
        for (_, infos, _) in self.0.iter_mut() {
            for shell_info in infos.iter_mut() {
                if shell_info.id == id || shell_info.menu.id == id {
                    task = shell_info.menu.close_if(menu_type.clone());
                }
            }
        }

        if esc_button_enabled && !self.menu_is_open() {
            Task::batch(vec![
                task,
                set_keyboard_interactivity(id, KeyboardInteractivity::None),
            ])
        } else {
            task
        }
    }

    pub fn close_all_menu_if<Message: 'static>(
        &mut self,
        menu_type: MenuType,
        esc_button_enabled: bool,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        for (_, infos, _) in self.0.iter_mut() {
            for shell_info in infos.iter_mut() {
                tasks.push(shell_info.menu.close_if(menu_type.clone()));
            }
        }
        let task = Task::batch(tasks);

        if esc_button_enabled && !self.menu_is_open() {
            let mut keyboard_tasks = Vec::new();
            for (_, infos, _) in self.0.iter() {
                for shell_info in infos {
                    keyboard_tasks.push(set_keyboard_interactivity(
                        shell_info.id,
                        KeyboardInteractivity::None,
                    ));
                }
            }
            Task::batch(vec![task, Task::batch(keyboard_tasks)])
        } else {
            task
        }
    }

    pub fn close_all_menus<Message: 'static>(&mut self, esc_button_enabled: bool) -> Task<Message> {
        let mut tasks = Vec::new();
        for (_, infos, _) in self.0.iter_mut() {
            for shell_info in infos.iter_mut() {
                if shell_info.menu.menu_info.is_some() {
                    tasks.push(shell_info.menu.close());
                }
            }
        }
        let task = Task::batch(tasks);

        if esc_button_enabled && !self.menu_is_open() {
            let mut keyboard_tasks = Vec::new();
            for (_, infos, _) in self.0.iter() {
                for shell_info in infos {
                    keyboard_tasks.push(set_keyboard_interactivity(
                        shell_info.id,
                        KeyboardInteractivity::None,
                    ));
                }
            }
            Task::batch(vec![task, Task::batch(keyboard_tasks)])
        } else {
            task
        }
    }

    pub fn request_keyboard<Message: 'static>(&self, id: Id) -> Task<Message> {
        for (_, infos, _) in self.0.iter() {
            for shell_info in infos {
                if shell_info.id == id || shell_info.menu.id == id {
                    return shell_info.menu.request_keyboard();
                }
            }
        }
        Task::none()
    }

    pub fn release_keyboard<Message: 'static>(&self, id: Id) -> Task<Message> {
        for (_, infos, _) in self.0.iter() {
            for shell_info in infos {
                if shell_info.id == id || shell_info.menu.id == id {
                    return shell_info.menu.release_keyboard();
                }
            }
        }
        Task::none()
    }
}
