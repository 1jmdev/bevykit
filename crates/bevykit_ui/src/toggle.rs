//! On/off switches.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::binding::{WidgetBinding, add_binding};
use crate::builder::{UiBuilder, WidgetBuilder};
use crate::focus::Focusable;
use crate::interaction::{Activated, OnValueChange, Pressable, ValueChanged};
use crate::state::{WidgetState, WidgetVisuals};
use crate::text::{ThemedText, UiText};

/// A switch holding a boolean value. Activating it flips the value.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(Pressable, Focusable, WidgetState)]
pub struct Toggle {
    /// The current value.
    pub value: bool,
    knob: Option<Entity>,
}

/// Builder for a toggle. Returned by [`UiBuilder::toggle`].
pub struct ToggleBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for ToggleBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl ToggleBuilder<'_> {
    /// Sets the initial value.
    pub fn value(&mut self, value: bool) -> &mut Self {
        self.entity
            .entry::<Toggle>()
            .and_modify(move |mut toggle| toggle.value = value);
        self
    }

    /// Writes the message produced by `message` whenever the player flips the toggle.
    pub fn on_change<M: Message>(
        &mut self,
        message: impl Fn(bool) -> M + Send + Sync + 'static,
    ) -> &mut Self {
        self.entity
            .entry::<OnValueChange<bool>>()
            .or_default()
            .and_modify(move |mut callbacks| {
                callbacks.push(move |commands, value| {
                    commands.write_message(message(value));
                });
            });
        self
    }

    /// Follows a value computed from resource `R`.
    pub fn bind_resource<R: Resource>(
        &mut self,
        read: impl Fn(&R) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            &mut self.entity,
            WidgetBinding::resource(read, |entity, value: bool| {
                if let Some(mut toggle) = entity.get_mut::<Toggle>() {
                    toggle.value = value;
                }
            }),
        );
        self
    }
}

pub(crate) fn flip_on_activate(
    activated: On<Activated>,
    mut toggles: Query<&mut Toggle>,
    mut commands: Commands,
) {
    if let Ok(mut toggle) = toggles.get_mut(activated.entity) {
        toggle.value = !toggle.value;
        commands.trigger(ValueChanged {
            entity: activated.entity,
            value: toggle.value,
        });
    }
}

pub(crate) fn update_toggle_visuals(
    mut toggles: Query<(&Toggle, &mut WidgetState), Changed<Toggle>>,
    mut knobs: Query<&mut Node>,
) {
    for (toggle, mut state) in &mut toggles {
        state.selected = toggle.value;
        if let Some(mut node) = toggle.knob.and_then(|knob| knobs.get_mut(knob).ok()) {
            node.left = if toggle.value {
                Val::Percent(50.0)
            } else {
                Val::Px(0.0)
            };
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a labeled on/off switch.
    pub fn toggle(&mut self, text: impl Into<UiText>) -> ToggleBuilder<'_> {
        let theme = self.theme;
        let height = theme.control_height * 0.6;
        let row = self
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: theme.gap(1.5),
                min_height: Val::Px(theme.control_height),
                ..default()
            })
            .id();
        let mut label = self
            .commands
            .spawn((ThemedText::body(), Pickable::IGNORE, ChildOf(row)));
        text.into().insert(&mut label);

        let track = self
            .commands
            .spawn((
                Node {
                    width: Val::Px(height * 2.0),
                    height: Val::Px(height),
                    padding: UiRect::all(Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(height)),
                    ..default()
                },
                WidgetVisuals::Secondary,
                ChildOf(row),
            ))
            .id();
        let knob = self
            .commands
            .spawn((
                Node {
                    width: Val::Percent(50.0),
                    height: Val::Percent(100.0),
                    left: Val::Px(0.0),
                    border_radius: BorderRadius::all(Val::Px(height)),
                    ..default()
                },
                BackgroundColor(theme.palette.text),
                Pickable::IGNORE,
                ChildOf(track),
            ))
            .id();
        self.commands.entity(track).insert(Toggle {
            value: false,
            knob: Some(knob),
        });
        ToggleBuilder {
            entity: self.commands.entity(track),
        }
    }
}
