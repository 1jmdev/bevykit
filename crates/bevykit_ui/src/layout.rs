//! Layout containers: rows, columns, spacers, and separators.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::builder::{UiBuilder, Widget};

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a horizontal container and fills it with `build`.
    pub fn row(&mut self, build: impl FnOnce(&mut UiBuilder)) -> Entity {
        let gap = self.theme.gap(1.0);
        self.container(
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: gap,
                ..default()
            },
            build,
        )
    }

    /// Adds a vertical container and fills it with `build`.
    pub fn column(&mut self, build: impl FnOnce(&mut UiBuilder)) -> Entity {
        let gap = self.theme.gap(1.0);
        self.container(
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: gap,
                ..default()
            },
            build,
        )
    }

    /// Adds a container with a custom layout and fills it with `build`.
    pub fn container(&mut self, node: Node, build: impl FnOnce(&mut UiBuilder)) -> Entity {
        let entity = self.spawn((node, Pickable::IGNORE)).id();
        self.within(entity, build);
        entity
    }

    /// Adds flexible empty space that pushes its siblings apart.
    pub fn spacer(&mut self) -> Widget<'_> {
        self.widget((
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Pickable::IGNORE,
        ))
    }

    /// Adds a thin horizontal line.
    pub fn separator(&mut self) -> Widget<'_> {
        let color = self.theme.palette.border;
        let margin = self.theme.gap(0.5);
        self.widget((
            Node {
                height: Val::Px(1.0),
                width: Val::Percent(100.0),
                margin: UiRect::vertical(margin),
                ..default()
            },
            BackgroundColor(color),
            Pickable::IGNORE,
        ))
    }
}
