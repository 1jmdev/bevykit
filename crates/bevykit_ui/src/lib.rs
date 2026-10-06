#![doc = include_str!("../README.md")]

extern crate self as bevykit_ui;

pub mod actions;
pub mod anchor;
pub mod binding;
pub mod builder;
pub mod button;
pub mod focus;
pub mod geometry;
pub mod interaction;
pub mod label;
pub mod layout;
pub mod modal;
pub mod panel;
pub mod scroll;
pub mod state;
pub mod text;
pub mod theme;

use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevykit_core::schedule::KitSystems;
use bevykit_input::KitInputPlugin;

/// Commonly used items.
pub mod prelude {
    pub use crate::KitUiPlugin;
    pub use crate::actions::{UiAction, ui_context};
    pub use crate::anchor::{AnchorTarget, OffscreenBehavior, WorldAnchor};
    pub use crate::binding::{Binding, BindingExt};
    pub use crate::builder::{Ui, UiBuilder, Widget, WidgetBuilder};
    pub use crate::focus::{Focus, FocusId, FocusLinks, FocusTrap, Focusable, InitialFocus};
    pub use crate::interaction::{Activated, OnActivate, Pressable, ValueChanged};
    pub use crate::panel::{Dismissible, PanelClosed, PanelId, Panels};
    pub use crate::scroll::{ScrollMemory, ScrollView};
    pub use crate::state::{WidgetState, WidgetVisuals};
    pub use crate::text::{ThemedText, UiText};
    pub use crate::theme::{Palette, TextRole, Typography, UiTheme};
}

/// Installs widgets, themes, focus navigation, panels, bindings, and world anchors.
#[derive(Default)]
pub struct KitUiPlugin {
    /// The initial theme.
    pub theme: UiTheme,
}

use theme::UiTheme;

impl Plugin for KitUiPlugin {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        if !app.is_plugin_added::<KitInputPlugin<actions::UiAction>>() {
            app.add_plugins(KitInputPlugin::<actions::UiAction>::default());
        }
        if !app.world().contains_resource::<InputFocus>() {
            app.init_resource::<InputFocus>();
        }
        if !app.world().contains_resource::<InputFocusVisible>() {
            app.init_resource::<InputFocusVisible>();
        }

        text::build(app);
        state::build(app);

        app.insert_resource(self.theme.clone())
            .register_type::<UiTheme>()
            .init_resource::<panel::Panels>()
            .init_resource::<focus::FocusTraps>()
            .init_resource::<modal::ModalStack>()
            .init_resource::<scroll::ScrollMemory>()
            .init_resource::<actions::NavigationRepeat>()
            .init_resource::<actions::NavigationRequest>()
            .add_message::<panel::PanelClosed>()
            .add_plugins((
                interaction::ValueCallbacksPlugin::<bool>::default(),
                interaction::ValueCallbacksPlugin::<f32>::default(),
                interaction::ValueCallbacksPlugin::<usize>::default(),
                interaction::ValueCallbacksPlugin::<String>::default(),
            ))
            .add_observer(interaction::run_activate_callbacks)
            .add_observer(focus::on_trap_added)
            .add_observer(focus::on_trap_removed)
            .add_observer(modal::on_modal_added)
            .add_observer(modal::on_modal_removed)
            .add_observer(panel::forget_closed_panel)
            .add_observer(scroll::remember_scroll)
            .add_observer(scroll::schedule_scroll_restore)
            .configure_sets(
                PostUpdate,
                (
                    KitSystems::Bindings.before(UiSystems::Prepare),
                    KitSystems::Presentation.before(UiSystems::Layout),
                ),
            )
            .add_systems(PreStartup, actions::install_default_bindings)
            .add_systems(
                PreUpdate,
                (
                    actions::update_navigation_request,
                    scroll::apply_scroll_restore,
                    scroll::drag_scroll,
                    scroll::wheel_scroll,
                    interaction::update_hover,
                    interaction::update_presses,
                    focus::focus_on_press,
                    focus::navigate,
                    interaction::activate_focused,
                    panel::dismiss_on_cancel,
                    modal::dismiss_on_backdrop,
                    scroll::scroll_focus_into_view,
                )
                    .chain()
                    .in_set(KitSystems::Interaction),
            )
            .add_systems(
                PostUpdate,
                (
                    binding::evaluate_bindings,
                    focus::apply_initial_focus,
                    focus::sync_focused_state,
                )
                    .chain()
                    .in_set(KitSystems::Bindings)
                    .before(state::apply_widget_visuals),
            )
            .add_systems(
                PostUpdate,
                anchor::position_anchored_nodes.in_set(KitSystems::Presentation),
            );
    }
}
