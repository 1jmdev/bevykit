//! Buttons.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::builder::{UiBuilder, WidgetBuilder};
use crate::focus::Focusable;
use crate::interaction::{OnActivate, Pressable};
use crate::state::WidgetVisuals;
use crate::text::{ThemedText, UiText};

/// Builder for a button. Returned by [`UiBuilder::button`].
///
/// A button activates on release. A press cancels when the pointer leaves the button before
/// release, or when a scroll view takes over the pointer.
pub struct ButtonBuilder<'a> {
    entity: EntityCommands<'a>,
    label: Option<Entity>,
}

impl<'a> WidgetBuilder<'a> for ButtonBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl ButtonBuilder<'_> {
    /// Draws the button with the accent color, for the main action of a screen.
    pub fn primary(&mut self) -> &mut Self {
        self.entity.insert(WidgetVisuals::Primary);
        if let Some(label) = self.label {
            self.entity.commands().entity(label).insert(ThemedText {
                on_accent: true,
                ..default()
            });
        }
        self
    }

    /// Draws the button without a background until hovered.
    pub fn ghost(&mut self) -> &mut Self {
        self.entity.insert(WidgetVisuals::Ghost);
        self
    }

    /// Runs a callback when the button activates.
    pub fn on_activate(
        &mut self,
        callback: impl Fn(&mut Commands, Entity) + Send + Sync + 'static,
    ) -> &mut Self {
        self.entity
            .entry::<OnActivate>()
            .or_default()
            .and_modify(move |mut callbacks| callbacks.push(callback));
        self
    }

    /// Writes a message when the button activates.
    pub fn send<M: Message + Clone>(&mut self, message: M) -> &mut Self {
        self.on_activate(move |commands, _| {
            commands.write_message(message.clone());
        })
    }

    /// Triggers an event when the button activates.
    pub fn trigger<E>(&mut self, event: E) -> &mut Self
    where
        E: Event + Clone,
        for<'t> E::Trigger<'t>: Default,
    {
        self.on_activate(move |commands, _| commands.trigger(event.clone()))
    }
}

/// The node of a button-like control sized for touch.
pub(crate) fn control_node(theme: &crate::theme::UiTheme) -> Node {
    Node {
        min_height: Val::Px(theme.control_height),
        padding: UiRect::axes(theme.gap(2.0), theme.gap(1.0)),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border: UiRect::all(Val::Px(theme.border_width)),
        border_radius: BorderRadius::all(Val::Px(theme.radius)),
        ..default()
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a button with a text label.
    pub fn button(&mut self, text: impl Into<UiText>) -> ButtonBuilder<'_> {
        let node = control_node(self.theme);
        let button = self
            .spawn((
                node,
                WidgetVisuals::Secondary,
                Pressable::default(),
                Focusable::default(),
                Button,
            ))
            .id();
        let mut label = self
            .commands
            .spawn((ThemedText::body(), Pickable::IGNORE, ChildOf(button)));
        text.into().insert(&mut label);
        let label = label.id();
        ButtonBuilder {
            entity: self.commands.entity(button),
            label: Some(label),
        }
    }

    /// Adds a button whose content is built by `build`, for icons or custom layouts.
    pub fn button_with(&mut self, build: impl FnOnce(&mut UiBuilder)) -> ButtonBuilder<'_> {
        let node = control_node(self.theme);
        let button = self
            .spawn((
                node,
                WidgetVisuals::Secondary,
                Pressable::default(),
                Focusable::default(),
                Button,
            ))
            .id();
        self.within(button, build);
        ButtonBuilder {
            entity: self.commands.entity(button),
            label: None,
        }
    }
}
