//! Progress bars.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::binding::{WidgetBinding, add_binding};
use crate::builder::{UiBuilder, WidgetBuilder};
use crate::state::WidgetVisuals;

/// A bar showing progress in `0.0..=1.0`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct ProgressBar {
    /// The shown progress.
    pub value: f32,
    fill: Option<Entity>,
}

/// Builder for a progress bar. Returned by [`UiBuilder::progress`].
pub struct ProgressBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for ProgressBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl ProgressBuilder<'_> {
    /// Sets the shown progress.
    pub fn value(&mut self, value: f32) -> &mut Self {
        self.entity
            .entry::<ProgressBar>()
            .and_modify(move |mut bar| bar.value = value);
        self
    }

    /// Follows progress computed from resource `R`.
    pub fn bind_resource<R: Resource>(
        &mut self,
        read: impl Fn(&R) -> f32 + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            &mut self.entity,
            WidgetBinding::resource(read, |entity, value: f32| {
                if let Some(mut bar) = entity.get_mut::<ProgressBar>() {
                    bar.value = value;
                }
            }),
        );
        self
    }
}

pub(crate) fn update_progress_fill(
    bars: Query<&ProgressBar, Changed<ProgressBar>>,
    mut nodes: Query<&mut Node>,
) {
    for bar in &bars {
        if let Some(mut node) = bar.fill.and_then(|fill| nodes.get_mut(fill).ok()) {
            node.width = Val::Percent(bar.value.clamp(0.0, 1.0) * 100.0);
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a progress bar.
    pub fn progress(&mut self) -> ProgressBuilder<'_> {
        let theme = self.theme;
        let height = theme.spacing;
        let track = self
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(height),
                    border_radius: BorderRadius::all(Val::Px(height)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                WidgetVisuals::Track,
            ))
            .id();
        let fill = self
            .commands
            .spawn((
                Node {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    border_radius: BorderRadius::all(Val::Px(height)),
                    ..default()
                },
                WidgetVisuals::Fill,
                Pickable::IGNORE,
                ChildOf(track),
            ))
            .id();
        self.commands.entity(track).insert(ProgressBar {
            value: 0.0,
            fill: Some(fill),
        });
        ProgressBuilder {
            entity: self.commands.entity(track),
        }
    }
}
