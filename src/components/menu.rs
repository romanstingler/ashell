use crate::app::{self, App};
use crate::components::{self, ButtonUIRef};
use crate::config::{BarSurface, Position, Surface};
use crate::theme::{backdrop_color, surface_border, use_theme};
use iced::alignment::Vertical;
use iced::widget::container::Style;
use iced::{
    Anchor, Element, KeyboardInteractivity, Layer, LayerShellSettings, Length, OutputId, Padding,
    Pixels, SurfaceId, Task, Theme, destroy_layer_surface, new_layer_surface,
    set_keyboard_interactivity,
    widget::{blur_container, container},
};
use std::time::Duration;

pub const ANIMATION_DURATION: Duration = Duration::from_millis(100);

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum MenuType {
    Updates,
    Settings,
    Notifications,
    Tray(String),
    MediaPlayer,
    SystemInfo,
    Tempo,
    AudioTooltip,
    BluetoothTooltip,
    WifiTooltip,
    VpnTooltip,
    BatteryTooltip,
    PeripheralBatteryTooltip(usize),
    CustomTooltip(String),
}

#[derive(Clone, Debug)]
pub struct OpenMenu {
    pub id: SurfaceId,
    /// Fullscreen click-catcher below the menu, so clicks outside the menu
    /// still close it while the menu surface only accepts input on the menu
    /// rectangle (leaving on-screen keyboards underneath it usable).
    pub catcher_id: Option<SurfaceId>,
    pub menu_type: MenuType,
    pub button_ui_ref: ButtonUIRef,
}

impl MenuType {
    pub fn is_tooltip(&self) -> bool {
        matches!(
            self,
            MenuType::AudioTooltip
                | MenuType::BluetoothTooltip
                | MenuType::WifiTooltip
                | MenuType::VpnTooltip
                | MenuType::BatteryTooltip
                | MenuType::PeripheralBatteryTooltip(_)
                | MenuType::CustomTooltip(_)
        )
    }
}

#[derive(Clone, Debug)]
struct PendingOpen {
    menu_type: MenuType,
    button_ui_ref: ButtonUIRef,
    request_keyboard: bool,
    output_id: Option<OutputId>,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub open: Option<OpenMenu>,
    closing: bool,
    pending_open: Option<PendingOpen>,
    animations_enabled: bool,
}

impl Menu {
    pub fn new() -> Self {
        Self::with_animations(false)
    }

    pub fn with_animations(animations_enabled: bool) -> Self {
        Self {
            open: None,
            closing: false,
            pending_open: None,
            animations_enabled,
        }
    }

    pub fn set_animations_enabled(&mut self, enabled: bool) {
        self.animations_enabled = enabled;
    }

    pub fn surface_id(&self) -> Option<SurfaceId> {
        self.open.as_ref().map(|o| o.id)
    }

    pub fn catcher_id(&self) -> Option<SurfaceId> {
        self.open.as_ref().and_then(|o| o.catcher_id)
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn is_closing(&self) -> bool {
        self.closing
    }

    pub fn open(
        &mut self,
        menu_type: MenuType,
        button_ui_ref: ButtonUIRef,
        request_keyboard: bool,
        output_id: Option<OutputId>,
    ) -> Task<app::Message> {
        self.closing = false;
        self.pending_open = None;

        let keyboard_interactivity = if request_keyboard {
            KeyboardInteractivity::OnDemand
        } else {
            KeyboardInteractivity::None
        };

        // Fullscreen catcher on the top layer, below the overlay menu and any
        // on-screen keyboard. It keeps click-outside working once the menu
        // surface's input region is limited to the menu rectangle.
        let catcher = if menu_type.is_tooltip() {
            None
        } else {
            Some(new_layer_surface(LayerShellSettings {
                namespace: "ashell-menu-catcher-layer".to_string(),
                size: None,
                layer: Layer::Top,
                keyboard_interactivity: KeyboardInteractivity::None,
                output: output_id,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                ..Default::default()
            }))
        };

        let (menu_id, task) = new_layer_surface(LayerShellSettings {
            namespace: "ashell-menu-layer".to_string(),
            size: None,
            layer: Layer::Overlay,
            keyboard_interactivity,
            output: output_id,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            ..Default::default()
        });

        // Destroy any surface still alive so reusing this slot never leaks a layer.
        let destroy = match self.open.take() {
            Some(open) => {
                let mut tasks = vec![destroy_layer_surface(open.id)];
                if let Some(catcher_id) = open.catcher_id {
                    tasks.push(destroy_layer_surface(catcher_id));
                }
                Task::batch(tasks)
            }
            None => Task::none(),
        };

        let catcher_id = catcher.as_ref().map(|(id, _)| *id);
        self.open = Some(OpenMenu {
            id: menu_id,
            catcher_id,
            menu_type,
            button_ui_ref,
        });

        let mut tasks = vec![destroy, task];
        if let Some((_, catcher_task)) = catcher {
            tasks.push(catcher_task);
        }
        Task::batch(tasks)
    }

    /// Begin the close animation, firing `FinishCloseMenu` once it ends.
    pub fn close(&mut self) -> Task<app::Message> {
        let Some(open) = self.open.as_ref() else {
            return Task::none();
        };
        if self.closing {
            return Task::none();
        }
        self.closing = true;
        let id = open.id;
        if !self.animations_enabled {
            return Task::done(app::Message::FinishCloseMenu(id));
        }
        Task::perform(
            async move {
                tokio::time::sleep(ANIMATION_DURATION).await;
                id
            },
            app::Message::FinishCloseMenu,
        )
    }

    /// Destroy the surface after the close animation, opening any queued menu.
    pub fn finish_close(&mut self) -> Task<app::Message> {
        if !self.closing {
            return Task::none();
        }
        self.closing = false;
        if let Some(pending) = self.pending_open.take() {
            // open() destroys the still-alive surface before reusing the slot.
            return self.open(
                pending.menu_type,
                pending.button_ui_ref,
                pending.request_keyboard,
                pending.output_id,
            );
        }
        if let Some(open) = self.open.take() {
            let mut tasks = vec![destroy_layer_surface(open.id)];
            if let Some(catcher_id) = open.catcher_id {
                tasks.push(destroy_layer_surface(catcher_id));
            }
            Task::batch(tasks)
        } else {
            Task::none()
        }
    }

    pub fn toggle(
        &mut self,
        menu_type: MenuType,
        button_ui_ref: ButtonUIRef,
        request_keyboard: bool,
        output_id: Option<OutputId>,
    ) -> Task<app::Message> {
        // While a close animates, reopening the same type cancels it; a
        // different type queues for after the animation completes.
        if self.closing {
            if let Some(current) = self.open.as_ref()
                && current.menu_type == menu_type
            {
                self.closing = false;
                self.pending_open = None;
                return Task::none();
            }
            self.pending_open = Some(PendingOpen {
                menu_type,
                button_ui_ref,
                request_keyboard,
                output_id,
            });
            return Task::none();
        }

        let menu_is_tooltip = matches!(
            menu_type,
            MenuType::AudioTooltip
                | MenuType::BluetoothTooltip
                | MenuType::WifiTooltip
                | MenuType::VpnTooltip
                | MenuType::BatteryTooltip
                | MenuType::PeripheralBatteryTooltip(_)
                | MenuType::CustomTooltip(_)
        );
        match &mut self.open {
            None => self.open(menu_type, button_ui_ref, request_keyboard, output_id),
            Some(open) if open.menu_type == menu_type => {
                if menu_is_tooltip {
                    open.button_ui_ref = button_ui_ref;
                    Task::none()
                } else {
                    self.close()
                }
            }
            Some(open)
                if !matches!(
                    open.menu_type,
                    MenuType::AudioTooltip
                        | MenuType::BluetoothTooltip
                        | MenuType::WifiTooltip
                        | MenuType::VpnTooltip
                        | MenuType::BatteryTooltip
                        | MenuType::PeripheralBatteryTooltip(_)
                        | MenuType::CustomTooltip(_)
                ) && menu_is_tooltip =>
            {
                Task::none()
            }
            // Tooltips and regular menus differ in whether they have a
            // catcher and a limited input region, so recreate the surface when
            // switching between them.
            Some(open) if open.menu_type.is_tooltip() != menu_is_tooltip => {
                self.pending_open = Some(PendingOpen {
                    menu_type,
                    button_ui_ref,
                    request_keyboard,
                    output_id,
                });
                self.close()
            }
            Some(open) => {
                open.menu_type = menu_type;
                open.button_ui_ref = button_ui_ref;
                Task::none()
            }
        }
    }

    pub fn close_if(&mut self, menu_type: MenuType) -> Task<app::Message> {
        if self.open.as_ref().is_some_and(|o| o.menu_type == menu_type) {
            self.close()
        } else {
            Task::none()
        }
    }

    pub fn request_keyboard<Message: 'static>(&self) -> Task<Message> {
        if let Some(open) = &self.open {
            // Exclusive grabs keyboard focus on Hyprland immediately, so keystrokes
            // reach the dialog without requiring a mouse click first.
            set_keyboard_interactivity(open.id, KeyboardInteractivity::Exclusive)
        } else {
            Task::none()
        }
    }

    pub fn release_keyboard<Message: 'static>(&self) -> Task<Message> {
        if let Some(open) = &self.open {
            set_keyboard_interactivity(open.id, KeyboardInteractivity::None)
        } else {
            Task::none()
        }
    }
}

#[allow(unused)]
pub enum MenuSize {
    Small,
    Medium,
    Large,
    XLarge,
}

impl MenuSize {
    pub fn size(&self) -> f32 {
        match self {
            MenuSize::Small => 250.,
            MenuSize::Medium => 350.,
            MenuSize::Large => 450.,
            MenuSize::XLarge => 650.,
        }
    }
}

impl From<MenuSize> for Length {
    fn from(value: MenuSize) -> Self {
        Length::Fixed(value.size())
    }
}

impl From<MenuSize> for Pixels {
    fn from(value: MenuSize) -> Self {
        Pixels::from(value.size())
    }
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn menu_wrapper<'a>(
        &'a self,
        id: SurfaceId,
        content: Element<'a, app::Message>,
        button_ui_ref: ButtonUIRef,
    ) -> Element<'a, app::Message> {
        let (space, radius, bar_surface, bar_position, menu_backdrop, blur) = use_theme(|t| {
            (
                t.space,
                t.radius,
                t.bar_layout.surface,
                t.bar_position,
                t.menu.backdrop,
                t.surface(Surface::Menu).blur,
            )
        });

        let menu_style = move |theme: &Theme| Style {
            background: Some(theme.palette().background.into()),
            border: surface_border(theme, radius.lg),
            ..Default::default()
        };
        let menu_body = if blur {
            blur_container(content)
                .padding(space.md)
                .style(menu_style)
                .width(Length::Shrink)
                .into()
        } else {
            container(content)
                .padding(space.md)
                .style(menu_style)
                .width(Length::Shrink)
                .into()
        };

        let wrapper = components::MenuWrapper::new(button_ui_ref.position.x, menu_body)
            .padding({
                let v_padding = match bar_surface {
                    BarSurface::Solid => 2,
                    BarSurface::Transparent => 0,
                };

                Padding::new(0.)
                    .top(if bar_position == Position::Top {
                        v_padding
                    } else {
                        0
                    })
                    .bottom(if bar_position == Position::Bottom {
                        v_padding
                    } else {
                        0
                    })
            })
            .align_y(match bar_position {
                Position::Top => Vertical::Top,
                Position::Bottom => Vertical::Bottom,
            })
            .backdrop(backdrop_color(menu_backdrop))
            .on_click_outside(app::Message::CloseMenu(id))
            .open(!self.outputs.menu_is_closing(id))
            .animated(use_theme(|t| t.animations_enabled));

        // Tooltips keep the fullscreen surface (no catcher, no input region);
        // regular menus publish their rectangle so the surface input region can
        // follow it and leave the rest of the output to the compositor.
        if self.outputs.menu_is_tooltip(id) {
            wrapper.into()
        } else {
            wrapper
                .on_bounds_changed(move |bounds| app::Message::MenuBoundsChanged(id, bounds))
                .into()
        }
    }
}
