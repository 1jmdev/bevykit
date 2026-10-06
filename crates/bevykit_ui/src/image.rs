//! Images supplied by the game.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::builder::{UiBuilder, Widget};

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds an image of the given logical size.
    pub fn image(&mut self, image: Handle<Image>, size: Vec2) -> Widget<'_> {
        self.widget((
            Node {
                width: Val::Px(size.x),
                height: Val::Px(size.y),
                ..default()
            },
            ImageNode::new(image),
            Pickable::IGNORE,
        ))
    }
}
