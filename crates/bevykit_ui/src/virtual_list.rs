//! Virtualized lists: only rows near the visible area exist as entities.
//!
//! ```ignore
//! ui.virtual_list(inventory.len(), 56.0, |ui, index| {
//!     ui.label(format!("Item {index}"));
//! });
//! ```
//!
//! Rows are spawned as they scroll into view and despawned as they leave it. Rows must have
//! the fixed height given to the list.

use std::sync::Arc;

use bevy::picking::Pickable;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::ComputedNode;

use crate::builder::UiBuilder;
use crate::scroll::ScrollView;
use crate::theme::UiTheme;

type RowBuilder = Arc<dyn Fn(&mut UiBuilder, usize) + Send + Sync>;

/// A scroll view whose rows are created on demand.
#[derive(Component, Clone)]
#[require(ScrollView)]
pub struct VirtualList {
    /// The number of rows.
    pub count: usize,
    /// The height of every row, in logical pixels.
    pub row_height: f32,
    /// Rows kept alive above and below the visible area.
    pub overscan: usize,
    build_row: RowBuilder,
    content: Option<Entity>,
    rows: HashMap<usize, Entity>,
    rebuild: bool,
    content_height: Option<f32>,
}

impl VirtualList {
    /// Changes the number of rows. Existing rows beyond the new count are removed.
    pub fn set_count(&mut self, count: usize) {
        self.count = count;
    }

    /// Rebuilds every visible row, for when the underlying data changed.
    pub fn refresh(&mut self) {
        self.rebuild = true;
    }
}

pub(crate) fn update_virtual_lists(
    mut commands: Commands,
    theme: Res<UiTheme>,
    mut lists: Query<(Entity, &mut VirtualList, &ScrollPosition, &ComputedNode)>,
    existing: Query<(), With<ChildOf>>,
) {
    for (entity, mut list, scroll, node) in &mut lists {
        let list = &mut *list;
        if list.rebuild {
            list.rebuild = false;
            for (_, row) in list.rows.drain() {
                commands.entity(row).try_despawn();
            }
        }
        let content = match list.content.filter(|content| existing.contains(*content)) {
            Some(content) => content,
            None => {
                let content = commands
                    .spawn((Node::default(), Pickable::IGNORE, ChildOf(entity)))
                    .id();
                list.content = Some(content);
                list.content_height = None;
                content
            }
        };
        let total_height = list.row_height * list.count as f32;
        if list.content_height != Some(total_height) {
            list.content_height = Some(total_height);
            commands.entity(content).insert(Node {
                width: Val::Percent(100.0),
                height: Val::Px(total_height),
                flex_shrink: 0.0,
                ..default()
            });
        }

        let viewport_height = node.size().y * node.inverse_scale_factor();
        let row_height = list.row_height.max(1.0);
        let first = (scroll.y / row_height).floor().max(0.0) as usize;
        let visible = (viewport_height / row_height).ceil() as usize + 1;
        let start = first.saturating_sub(list.overscan);
        let end = (first + visible + list.overscan).min(list.count);

        let stale: Vec<usize> = list
            .rows
            .keys()
            .copied()
            .filter(|index| *index < start || *index >= end)
            .collect();
        for index in stale {
            if let Some(row) = list.rows.remove(&index) {
                commands.entity(row).try_despawn();
            }
        }

        for index in start..end {
            if list.rows.contains_key(&index) {
                continue;
            }
            let row = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(index as f32 * list.row_height),
                        width: Val::Percent(100.0),
                        height: Val::Px(list.row_height),
                        ..default()
                    },
                    Pickable::IGNORE,
                    ChildOf(content),
                ))
                .id();
            let mut builder = UiBuilder::new(&mut commands, &theme, row);
            (list.build_row)(&mut builder, index);
            list.rows.insert(index, row);
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a virtualized list of `count` rows of `row_height` logical pixels.
    pub fn virtual_list(
        &mut self,
        count: usize,
        row_height: f32,
        build_row: impl Fn(&mut UiBuilder, usize) + Send + Sync + 'static,
    ) -> Entity {
        self.spawn(VirtualList {
            count,
            row_height,
            overscan: 3,
            build_row: Arc::new(build_row),
            content: None,
            rows: HashMap::default(),
            rebuild: false,
            content_height: None,
        })
        .id()
    }
}
