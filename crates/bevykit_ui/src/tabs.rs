//! Tabs: a row of tab buttons switching between pages.
//!
//! ```ignore
//! ui.tabs(|tabs| {
//!     tabs.tab(tr!("settings.audio"), |ui| build_audio(ui));
//!     tabs.tab(tr!("settings.video"), |ui| build_video(ui));
//! });
//! ```
//!
//! The next-tab and previous-tab actions switch the tabs containing the focused widget.

use bevy::input_focus::InputFocus;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevykit_input::prelude::*;

use crate::actions::UiAction;
use crate::builder::UiBuilder;
use crate::button::control_node;
use crate::focus::Focusable;
use crate::interaction::{Activated, Pressable, ValueChanged};
use crate::state::{WidgetState, WidgetVisuals};
use crate::text::{ThemedText, UiText};

/// A set of tabs.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Tabs {
    /// The selected tab.
    pub selected: usize,
    buttons: Vec<Entity>,
    pages: Vec<Entity>,
}

impl Tabs {
    /// Returns the number of tabs.
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Returns `true` if there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

/// A tab button, pointing at its [`Tabs`].
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct TabButton {
    tabs: Entity,
    index: usize,
}

/// Adds tabs to a [`Tabs`] container. Passed to the closure of [`UiBuilder::tabs`].
pub struct TabsBuilder<'b, 'a, 'w, 's> {
    ui: &'b mut UiBuilder<'a, 'w, 's>,
    tabs: Entity,
    bar: Entity,
    pages: Entity,
    buttons: Vec<Entity>,
    page_entities: Vec<Entity>,
}

impl TabsBuilder<'_, '_, '_, '_> {
    /// Adds a tab with a title and content.
    pub fn tab(&mut self, title: impl Into<UiText>, build: impl FnOnce(&mut UiBuilder)) -> &mut Self {
        let index = self.buttons.len();
        let theme = self.ui.theme;
        let button = self
            .ui
            .commands
            .spawn((
                control_node(theme),
                WidgetVisuals::Ghost,
                Pressable::default(),
                Focusable::default(),
                TabButton {
                    tabs: self.tabs,
                    index,
                },
                ChildOf(self.bar),
            ))
            .id();
        let mut label = self
            .ui
            .commands
            .spawn((ThemedText::body(), Pickable::IGNORE, ChildOf(button)));
        title.into().insert(&mut label);

        let page = self
            .ui
            .commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: theme.gap(1.0),
                    display: if index == 0 { Display::Flex } else { Display::None },
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(self.pages),
            ))
            .id();
        self.ui.within(page, build);
        self.buttons.push(button);
        self.page_entities.push(page);
        self
    }
}

pub(crate) fn select_on_activate(
    activated: On<Activated>,
    buttons: Query<&TabButton>,
    mut tabs: Query<&mut Tabs>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activated.entity) else {
        return;
    };
    if let Ok(mut tabs) = tabs.get_mut(button.tabs)
        && tabs.selected != button.index
    {
        tabs.selected = button.index;
        commands.trigger(ValueChanged {
            entity: button.tabs,
            value: button.index,
        });
    }
}

pub(crate) fn cycle_tabs_with_actions(
    mut actions: ResMut<ActionState<UiAction>>,
    focus: Res<InputFocus>,
    parents: Query<&ChildOf>,
    mut tabs: Query<(Entity, &mut Tabs)>,
    mut commands: Commands,
) {
    let step: isize = if actions.just_pressed(UiAction::NextTab) {
        1
    } else if actions.just_pressed(UiAction::PreviousTab) {
        -1
    } else {
        return;
    };
    let mut current = focus.get();
    while let Some(entity) = current {
        if let Ok((entity, mut tabs)) = tabs.get_mut(entity) {
            if tabs.is_empty() {
                return;
            }
            let count = tabs.len() as isize;
            tabs.selected = (tabs.selected as isize + step).rem_euclid(count) as usize;
            commands.trigger(ValueChanged {
                entity,
                value: tabs.selected,
            });
            actions.consume(UiAction::NextTab);
            actions.consume(UiAction::PreviousTab);
            return;
        }
        current = parents.get(entity).ok().map(ChildOf::parent);
    }
}

pub(crate) fn show_selected_page(
    tabs: Query<&Tabs, Changed<Tabs>>,
    mut nodes: Query<&mut Node>,
    mut states: Query<&mut WidgetState>,
) {
    for tabs in &tabs {
        for (index, page) in tabs.pages.iter().enumerate() {
            if let Ok(mut node) = nodes.get_mut(*page) {
                let display = if index == tabs.selected {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display != display {
                    node.display = display;
                }
            }
        }
        for (index, button) in tabs.buttons.iter().enumerate() {
            if let Ok(mut state) = states.get_mut(*button) {
                state.selected = index == tabs.selected;
            }
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds tabs. Returns the [`Tabs`] entity, which triggers [`ValueChanged<usize>`] when the
    /// selection changes.
    pub fn tabs(&mut self, build: impl FnOnce(&mut TabsBuilder)) -> Entity {
        let theme = self.theme;
        let tabs = self
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: theme.gap(1.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .id();
        let bar = self
            .commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: theme.gap(0.5),
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(tabs),
            ))
            .id();
        let pages = self
            .commands
            .spawn((Node::default(), Pickable::IGNORE, ChildOf(tabs)))
            .id();
        let mut builder = TabsBuilder {
            ui: self,
            tabs,
            bar,
            pages,
            buttons: Vec::new(),
            page_entities: Vec::new(),
        };
        build(&mut builder);
        let component = Tabs {
            selected: 0,
            buttons: builder.buttons,
            pages: builder.page_entities,
        };
        self.commands.entity(tabs).insert(component);
        tabs
    }
}
