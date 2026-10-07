//! Notifications: brief messages stacked at the top of the screen.

use std::collections::VecDeque;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::GlobalZIndex;
use bevykit_core::display::SafeAreaInsets;

use crate::lifetime::Lifetime;
use crate::state::WidgetVisuals;
use crate::text::{ThemedText, UiText};
use crate::theme::UiTheme;

/// Notification layout and timing.
#[derive(Resource, Clone, Debug)]
pub struct NotificationSettings {
    /// Seconds each notification stays visible.
    pub duration: f32,
    /// Notifications shown at once; the rest wait.
    pub max_visible: usize,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            duration: 3.0,
            max_visible: 3,
        }
    }
}

/// Notifications waiting to be shown.
#[derive(Resource, Default, Debug)]
pub struct NotificationQueue {
    pending: VecDeque<UiText>,
    container: Option<Entity>,
}

impl NotificationQueue {
    /// Queues a notification.
    pub fn push(&mut self, text: UiText) {
        self.pending.push_back(text);
    }

    /// Returns the number of notifications not yet shown.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

/// Marks a visible notification.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Notification;

pub(crate) fn show_notifications(
    mut queue: ResMut<NotificationQueue>,
    settings: Res<NotificationSettings>,
    theme: Res<UiTheme>,
    insets: Option<Res<SafeAreaInsets>>,
    visible: Query<(), With<Notification>>,
    existing: Query<(), With<Node>>,
    mut commands: Commands,
) {
    if queue.pending.is_empty() {
        return;
    }
    let top = insets.map(|insets| insets.top).unwrap_or_default();
    let container = match queue.container.filter(|entity| existing.contains(*entity)) {
        Some(container) => container,
        None => {
            let container = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        top: Val::Px(top + theme.spacing * 2.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: theme.gap(1.0),
                        ..default()
                    },
                    GlobalZIndex(i32::MAX - 20),
                    Pickable::IGNORE,
                    Name::new("Notifications"),
                ))
                .id();
            queue.container = Some(container);
            container
        }
    };

    let mut shown = visible.iter().count();
    while shown < settings.max_visible {
        let Some(text) = queue.pending.pop_front() else {
            break;
        };
        let toast = commands
            .spawn((
                Node {
                    padding: UiRect::axes(theme.gap(2.0), theme.gap(1.0)),
                    border: UiRect::all(Val::Px(theme.border_width)),
                    border_radius: BorderRadius::all(Val::Px(theme.radius)),
                    max_width: Val::Percent(90.0),
                    ..default()
                },
                WidgetVisuals::Surface,
                Notification,
                Lifetime::seconds(settings.duration),
                Pickable::IGNORE,
                ChildOf(container),
            ))
            .id();
        let mut label = commands.spawn((ThemedText::body(), Pickable::IGNORE, ChildOf(toast)));
        text.insert(&mut label);
        shown += 1;
    }
}
