//! Panels: named screens that open, close, and replace one another.

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevykit_core::define_key;
use bevykit_input::prelude::*;

use crate::actions::UiAction;
use crate::anchor::WorldAnchor;
use crate::builder::{Ui, UiBuilder};
use crate::focus::FocusTrap;
use crate::modal::ModalLayer;
use crate::scroll::{ScrollMemoryKey, ScrollView};
use crate::state::WidgetVisuals;
use crate::text::{ThemedText, UiText};
use crate::theme::TextRole;

define_key!(
    /// Names a panel.
    PanelId
);

/// Marks the outermost entity of an open panel.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct PanelRoot {
    /// The panel's identifier.
    pub id: PanelId,
}

/// Closes the panel when the player cancels while it is the topmost dismissible panel.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Dismissible;

/// Sent when a panel closes, by code or by the player.
#[derive(Message, Clone, Copy, Debug)]
pub struct PanelClosed(pub PanelId);

/// Open panels in the order they were opened.
#[derive(Resource, Default, Debug)]
pub struct Panels {
    open: Vec<(PanelId, Entity)>,
}

impl Panels {
    /// Returns the root entity of an open panel.
    pub fn entity(&self, id: PanelId) -> Option<Entity> {
        self.open
            .iter()
            .find(|(open, _)| *open == id)
            .map(|(_, entity)| *entity)
    }

    /// Iterates open panels, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (PanelId, Entity)> + '_ {
        self.open.iter().copied()
    }
}

/// Where a panel appears.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Placement {
    #[default]
    Center,
    Anchored,
}

/// Describes a panel. Returned by [`Ui::panel`].
pub struct PanelBuilder<'u, 'w, 's> {
    ui: &'u mut Ui<'w, 's>,
    id: PanelId,
    title: Option<UiText>,
    modal: bool,
    dismissible: bool,
    scrollable: bool,
    preserve_scroll: bool,
    width: Val,
    max_height: Val,
    anchor: Option<WorldAnchor>,
}

impl<'u, 'w, 's> PanelBuilder<'u, 'w, 's> {
    pub(crate) fn new(ui: &'u mut Ui<'w, 's>, id: PanelId) -> Self {
        Self {
            ui,
            id,
            title: None,
            modal: false,
            dismissible: false,
            scrollable: false,
            preserve_scroll: false,
            width: Val::Auto,
            max_height: Val::Percent(85.0),
            anchor: None,
        }
    }

    /// Shows a title above the content.
    pub fn title(mut self, title: impl Into<UiText>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Dims everything behind the panel, captures pointer input and focus, and blocks the
    /// gameplay input context while open. Modal panels are also dismissible.
    pub fn modal(mut self) -> Self {
        self.modal = true;
        self.dismissible = true;
        self
    }

    /// Closes the panel when the player cancels.
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Scrolls the content when it is taller than the panel.
    pub fn scrollable(mut self) -> Self {
        self.scrollable = true;
        self
    }

    /// Restores the scroll position when the panel is opened again.
    pub fn preserve_scroll(mut self) -> Self {
        self.scrollable = true;
        self.preserve_scroll = true;
        self
    }

    /// Sets the panel width.
    pub fn width(mut self, width: Val) -> Self {
        self.width = width;
        self
    }

    /// Sets the largest height before the content scrolls or clips.
    pub fn max_height(mut self, height: Val) -> Self {
        self.max_height = height;
        self
    }

    /// Positions the panel relative to an entity or world location.
    pub fn anchor(mut self, anchor: WorldAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Offsets an anchored panel on screen, in logical pixels.
    pub fn screen_offset(mut self, offset: Vec2) -> Self {
        if let Some(anchor) = &mut self.anchor {
            anchor.screen_offset = offset;
        }
        self
    }

    /// Keeps an anchored panel inside the safe area.
    pub fn clamp_to_safe_area(mut self) -> Self {
        if let Some(anchor) = &mut self.anchor {
            anchor.clamp_to_safe_area = true;
        }
        self
    }

    /// Spawns the panel and runs `build` to add its content. Returns the panel's root entity.
    pub fn build(self, build: impl FnOnce(&mut UiBuilder)) -> Entity {
        let Self {
            ui,
            id,
            title,
            modal,
            dismissible,
            scrollable,
            preserve_scroll,
            width,
            max_height,
            anchor,
        } = self;

        if let Some(existing) = ui.panels.entity(id) {
            ui.commands.entity(existing).try_despawn();
            ui.panels.open.retain(|(open, _)| *open != id);
        }
        let theme = ui.theme.clone();
        let placement = if anchor.is_some() {
            Placement::Anchored
        } else {
            Placement::Center
        };

        let panel = ui
            .commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(theme.gap(2.0)),
                    row_gap: theme.gap(1.0),
                    width,
                    max_height,
                    border: UiRect::all(Val::Px(theme.border_width)),
                    border_radius: BorderRadius::all(Val::Px(theme.radius * 1.5)),
                    ..default()
                },
                WidgetVisuals::Surface,
                Name::new(format!("Panel {:?}", id.key())),
            ))
            .id();

        let root = if modal {
            let backdrop = ui
                .commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(theme.palette.backdrop),
                    ModalLayer::new(panel),
                    FocusTrap,
                ))
                .id();
            ui.commands.entity(panel).insert(ChildOf(backdrop));
            backdrop
        } else if placement == Placement::Center {
            let wrapper = ui
                .commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .id();
            ui.commands.entity(panel).insert(ChildOf(wrapper));
            wrapper
        } else {
            ui.commands.entity(panel).insert(Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(theme.gap(1.5)),
                row_gap: theme.gap(1.0),
                width,
                max_height,
                border: UiRect::all(Val::Px(theme.border_width)),
                border_radius: BorderRadius::all(Val::Px(theme.radius * 1.5)),
                ..default()
            });
            if let Some(anchor) = anchor {
                ui.commands.entity(panel).insert(anchor);
            }
            panel
        };
        ui.commands.entity(root).insert(PanelRoot { id });
        if dismissible {
            ui.commands.entity(root).insert(Dismissible);
        }

        if let Some(title) = title {
            let mut title_entity = ui.commands.spawn((
                ThemedText::role(TextRole::Title),
                ChildOf(panel),
                Pickable::IGNORE,
            ));
            title.insert(&mut title_entity);
        }

        let content = if scrollable {
            let mut content = ui.commands.spawn((ScrollView::vertical(), ChildOf(panel)));
            content.entry::<Node>().and_modify({
                let gap = theme.gap(1.0);
                move |mut node| node.row_gap = gap
            });
            if preserve_scroll {
                content.insert(ScrollMemoryKey(id.key()));
            }
            content.id()
        } else {
            panel
        };

        let mut builder = UiBuilder::new(&mut ui.commands, &theme, content);
        build(&mut builder);
        ui.panels.open.push((id, root));
        root
    }
}

pub(crate) fn forget_closed_panel(
    removed: On<Remove, PanelRoot>,
    roots: Query<&PanelRoot>,
    mut panels: ResMut<Panels>,
    mut closed: MessageWriter<PanelClosed>,
) {
    let Ok(root) = roots.get(removed.entity) else {
        return;
    };
    let before = panels.open.len();
    panels.open.retain(|(_, entity)| *entity != removed.entity);
    if panels.open.len() != before {
        closed.write(PanelClosed(root.id));
    }
}

/// Closes the most recently opened dismissible panel when the player cancels.
pub(crate) fn dismiss_on_cancel(
    mut actions: ResMut<ActionState<UiAction>>,
    panels: Res<Panels>,
    dismissible: Query<(), With<Dismissible>>,
    mut commands: Commands,
) {
    if !actions.just_pressed(UiAction::Cancel) {
        return;
    }
    let topmost = panels
        .open
        .iter()
        .rev()
        .find(|(_, entity)| dismissible.contains(*entity));
    if let Some((_, entity)) = topmost {
        actions.consume(UiAction::Cancel);
        commands.entity(*entity).try_despawn();
    }
}
