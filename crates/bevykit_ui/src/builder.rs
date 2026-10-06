//! Composing screens: the [`Ui`] system parameter, the [`UiBuilder`] passed to build closures,
//! and the [`WidgetBuilder`] trait shared by every widget builder.
//!
//! Screens are composed once, when they open. Afterwards they update through bindings and
//! change-driven systems, so focus, scroll positions, and in-progress interactions survive.
//!
//! ```ignore
//! fn open_settings(mut ui: Ui, settings: Res<SettingsStore<GameSettings>>) {
//!     ui.panel(PanelId::new("settings"))
//!         .title(tr!("settings.title"))
//!         .modal()
//!         .build(|ui| {
//!             ui.slider(tr!("settings.music"))
//!                 .range(0.0..=1.0)
//!                 .value(settings.music_volume)
//!                 .on_change(SetMusicVolume);
//!
//!             ui.button(tr!("settings.close")).send(CloseSettings);
//!         });
//! }
//! ```

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::focus::{FocusId, FocusLinks, Focusable, InitialFocus};
use crate::panel::{PanelBuilder, PanelId, Panels};
use crate::state::WidgetState;
use crate::theme::UiTheme;

/// System parameter for opening panels and composing UI.
#[derive(SystemParam)]
pub struct Ui<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) theme: Res<'w, UiTheme>,
    pub(crate) panels: ResMut<'w, Panels>,
}

impl<'w, 's> Ui<'w, 's> {
    /// Starts describing a panel. Opening a panel that is already open replaces it.
    pub fn panel(&mut self, id: impl Into<PanelId>) -> PanelBuilder<'_, 'w, 's> {
        PanelBuilder::new(self, id.into())
    }

    /// Closes a panel. Closing a panel that is not open is a no-op.
    pub fn close(&mut self, id: impl Into<PanelId>) {
        if let Some(entity) = self.panels.entity(id.into()) {
            self.commands.entity(entity).try_despawn();
        }
    }

    /// Returns `true` if the panel is open.
    pub fn is_open(&self, id: impl Into<PanelId>) -> bool {
        self.panels.entity(id.into()).is_some()
    }

    /// Replaces the theme. Widgets restyle on the next frame.
    pub fn set_theme(&mut self, theme: UiTheme) {
        self.commands.insert_resource(theme);
    }

    /// Returns the active theme.
    pub fn theme(&self) -> &UiTheme {
        &self.theme
    }

    /// Adds widgets to an existing entity.
    pub fn build_in(&mut self, parent: Entity, build: impl FnOnce(&mut UiBuilder)) {
        let mut builder = UiBuilder {
            commands: &mut self.commands,
            theme: &self.theme,
            parent,
        };
        build(&mut builder);
    }

    /// Grants access to the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'w, 's> {
        &mut self.commands
    }
}

/// Adds widgets to a parent entity. Passed to build closures.
pub struct UiBuilder<'a, 'w, 's> {
    pub(crate) commands: &'a mut Commands<'w, 's>,
    pub(crate) theme: &'a UiTheme,
    pub(crate) parent: Entity,
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Creates a builder adding children to `parent`.
    pub fn new(commands: &'a mut Commands<'w, 's>, theme: &'a UiTheme, parent: Entity) -> Self {
        Self {
            commands,
            theme,
            parent,
        }
    }

    /// Returns the entity receiving new widgets.
    pub fn parent(&self) -> Entity {
        self.parent
    }

    /// Returns the active theme.
    pub fn theme(&self) -> &UiTheme {
        self.theme
    }

    /// Grants access to the underlying commands.
    pub fn commands(&mut self) -> &mut Commands<'w, 's> {
        self.commands
    }

    /// Spawns an entity as a child of the current parent.
    pub fn spawn(&mut self, bundle: impl Bundle) -> EntityCommands<'_> {
        let parent = self.parent;
        let mut entity = self.commands.spawn(bundle);
        entity.insert(ChildOf(parent));
        entity
    }

    /// Spawns a child and returns a generic widget builder for it.
    pub fn widget(&mut self, bundle: impl Bundle) -> Widget<'_> {
        Widget {
            entity: self.spawn(bundle),
        }
    }

    /// Runs `build` with `parent` as the parent of new widgets.
    pub fn within(&mut self, parent: Entity, build: impl FnOnce(&mut UiBuilder)) {
        let mut nested = UiBuilder {
            commands: &mut *self.commands,
            theme: self.theme,
            parent,
        };
        build(&mut nested);
    }
}

/// Common configuration available on every widget builder.
///
/// Feature modules add more methods through extension traits, such as
/// [`TooltipExt`](crate::tooltip::TooltipExt) and [`BindingExt`](crate::binding::BindingExt).
pub trait WidgetBuilder<'a>: Sized {
    /// Returns the commands of the widget's root entity.
    fn entity_commands(&mut self) -> &mut EntityCommands<'a>;

    /// Returns the widget's root entity.
    fn id(&mut self) -> Entity {
        self.entity_commands().id()
    }

    /// Inserts components on the widget's root entity.
    fn insert(&mut self, bundle: impl Bundle) -> &mut Self {
        self.entity_commands().insert(bundle);
        self
    }

    /// Adjusts the layout of the widget's root node.
    fn node(&mut self, adjust: impl FnOnce(&mut Node) + Send + Sync + 'static) -> &mut Self {
        self.entity_commands().entry::<Node>().and_modify(move |mut node| adjust(&mut node));
        self
    }

    /// Names the widget for focus links and programmatic focus.
    fn focus_id(&mut self, id: impl Into<FocusId>) -> &mut Self {
        let id = id.into();
        self.entity_commands()
            .entry::<Focusable>()
            .or_default()
            .and_modify(move |mut focusable| focusable.id = Some(id));
        self
    }

    /// Focuses the widget when its panel opens.
    fn initial_focus(&mut self) -> &mut Self {
        self.insert(InitialFocus)
    }

    /// Sets explicit focus navigation targets.
    fn focus_links(&mut self, links: FocusLinks) -> &mut Self {
        self.insert(links)
    }

    /// Enables or disables the widget.
    fn disabled(&mut self, disabled: bool) -> &mut Self {
        self.entity_commands()
            .entry::<WidgetState>()
            .or_default()
            .and_modify(move |mut state| state.disabled = disabled);
        self
    }

    /// Marks the widget as selected.
    fn selected(&mut self, selected: bool) -> &mut Self {
        self.entity_commands()
            .entry::<WidgetState>()
            .or_default()
            .and_modify(move |mut state| state.selected = selected);
        self
    }
}

/// A builder for widgets without specific configuration, such as containers and labels.
pub struct Widget<'a> {
    pub(crate) entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for Widget<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}
