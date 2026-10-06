//! Single-line text entry, built on Bevy's editable text.
//!
//! While a text field is focused, every other input context is blocked so that typing does not
//! also move the character or the focus. Enter submits the text; Escape removes focus.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextEditChange};
use bevykit_input::prelude::*;

use crate::builder::{UiBuilder, WidgetBuilder};
use crate::focus::Focusable;
use crate::interaction::{OnValueChange, ValueChanged};
use crate::state::WidgetVisuals;
use crate::text::ThemedText;

/// A text entry field.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component)]
#[require(Focusable)]
pub struct TextField {
    last_value: String,
}

/// Triggered on a text field when the player presses Enter.
#[derive(EntityEvent, Clone, Debug)]
pub struct TextSubmitted {
    /// The field.
    pub entity: Entity,
    /// The submitted text.
    pub value: String,
}

/// Builder for a text field. Returned by [`UiBuilder::text_field`].
pub struct TextFieldBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for TextFieldBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl TextFieldBuilder<'_> {
    /// Writes the message produced by `message` whenever the text changes.
    pub fn on_change<M: Message>(
        &mut self,
        message: impl Fn(String) -> M + Send + Sync + 'static,
    ) -> &mut Self {
        self.entity
            .entry::<OnValueChange<String>>()
            .or_default()
            .and_modify(move |mut callbacks| {
                callbacks.push(move |commands, value| {
                    commands.write_message(message(value));
                });
            });
        self
    }

    /// Writes the message produced by `message` when the player presses Enter.
    pub fn on_submit<M: Message>(
        &mut self,
        message: impl Fn(String) -> M + Send + Sync + 'static,
    ) -> &mut Self {
        self.entity
            .observe(move |submitted: On<TextSubmitted>, mut commands: Commands| {
                commands.write_message(message(submitted.value.clone()));
            });
        self
    }
}

/// Reports text changes as [`ValueChanged<String>`].
pub(crate) fn report_text_changes(
    change: On<TextEditChange>,
    mut fields: Query<(&mut TextField, &EditableText)>,
    mut commands: Commands,
) {
    let entity = change.event_target();
    let Ok((mut field, editable)) = fields.get_mut(entity) else {
        return;
    };
    let value = editable.value().to_string();
    if value != field.last_value {
        field.last_value = value.clone();
        commands.trigger(ValueChanged { entity, value });
    }
}

/// Handles Enter and Escape on the focused field.
pub(crate) fn submit_and_blur(
    mut keys: MessageReader<KeyboardInput>,
    mut focus: ResMut<InputFocus>,
    fields: Query<&EditableText, With<TextField>>,
    mut commands: Commands,
) {
    let Some(entity) = focus.get().filter(|entity| fields.contains(*entity)) else {
        keys.clear();
        return;
    };
    for key in keys.read() {
        if !key.state.is_pressed() {
            continue;
        }
        match key.logical_key {
            Key::Enter => {
                if let Ok(editable) = fields.get(entity) {
                    commands.trigger(TextSubmitted {
                        entity,
                        value: editable.value().to_string(),
                    });
                }
            }
            Key::Escape => {
                focus.clear();
                return;
            }
            _ => {}
        }
    }
}

/// Blocks every other input context while a text field is focused.
pub(crate) fn capture_input_while_editing(
    focus: Res<InputFocus>,
    fields: Query<(), With<TextField>>,
    mut contexts: ResMut<InputContexts>,
) {
    let editing = focus.get().is_some_and(|entity| fields.contains(entity));
    let blocking = contexts.contains(&InputContext::TextEntry);
    if editing && !blocking {
        contexts.push(InputContext::TextEntry).block_all();
    } else if !editing && blocking {
        contexts.remove(&InputContext::TextEntry);
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a text field with initial text.
    pub fn text_field(&mut self, initial: impl Into<String>) -> TextFieldBuilder<'_> {
        let theme = self.theme;
        let initial = initial.into();
        let entity = self.spawn((
            Node {
                min_height: Val::Px(theme.control_height),
                padding: UiRect::axes(theme.gap(1.5), theme.gap(1.0)),
                border: UiRect::all(Val::Px(theme.border_width)),
                border_radius: BorderRadius::all(Val::Px(theme.radius)),
                width: Val::Percent(100.0),
                ..default()
            },
            EditableText::new(&initial),
            ThemedText::body(),
            WidgetVisuals::Track,
            TextField {
                last_value: initial,
            },
        ));
        TextFieldBuilder { entity }
    }
}
