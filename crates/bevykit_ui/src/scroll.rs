//! Scroll views: wheel, drag, and momentum scrolling, with remembered positions.
//!
//! Dragging a scroll view captures the pointer once it travels past
//! [`ScrollView::drag_threshold`]. Capturing cancels any button press inside, so a press that
//! turns into a scroll never activates the button under the finger.

use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::picking::pointer::PointerId;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevykit_core::key::Key;
use bevykit_input::prelude::*;

/// Which axes a scroll view scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ScrollAxes {
    /// Vertical only.
    #[default]
    Vertical,
    /// Horizontal only.
    Horizontal,
    /// Both axes.
    Both,
}

impl ScrollAxes {
    fn mask(&self) -> Vec2 {
        match self {
            ScrollAxes::Vertical => Vec2::Y,
            ScrollAxes::Horizontal => Vec2::X,
            ScrollAxes::Both => Vec2::ONE,
        }
    }
}

/// A node whose content scrolls.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
#[require(Node = scroll_node(), ScrollPosition)]
pub struct ScrollView {
    /// The scrolling axes.
    pub axes: ScrollAxes,
    /// Pointer travel, in logical pixels, before a press becomes a drag.
    pub drag_threshold: f32,
    /// Logical pixels per wheel line.
    pub line_height: f32,
    /// Fraction of momentum kept per second after a drag is released.
    pub friction: f32,
    #[reflect(ignore)]
    candidate: Option<PointerId>,
    #[reflect(ignore)]
    dragging: Option<PointerId>,
    velocity: Vec2,
}

fn scroll_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        overflow: Overflow::scroll_y(),
        min_height: Val::Px(0.0),
        ..default()
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        Self {
            axes: ScrollAxes::Vertical,
            drag_threshold: 8.0,
            line_height: 32.0,
            friction: 0.05,
            candidate: None,
            dragging: None,
            velocity: Vec2::ZERO,
        }
    }
}

impl ScrollView {
    /// A vertical scroll view.
    pub fn vertical() -> Self {
        Self::default()
    }

    /// A horizontal scroll view. Also set the node's overflow with [`Overflow::scroll_x`].
    pub fn horizontal() -> Self {
        Self {
            axes: ScrollAxes::Horizontal,
            ..default()
        }
    }

    /// Returns `true` while the view is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }
}

/// Remembers the scroll position of a view across closing and reopening, under this key.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct ScrollMemoryKey(pub Key);

/// Remembered scroll positions.
#[derive(Resource, Default, Debug)]
pub struct ScrollMemory {
    positions: HashMap<Key, Vec2>,
}

impl ScrollMemory {
    /// Returns the remembered position for a key.
    pub fn get(&self, key: Key) -> Option<Vec2> {
        self.positions.get(&key).copied()
    }

    /// Forgets every remembered position.
    pub fn clear(&mut self) {
        self.positions.clear();
    }
}

/// A scroll position waiting for layout before it can be applied.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PendingScrollRestore(Vec2);

fn max_scroll(node: &ComputedNode) -> Vec2 {
    ((node.content_size() - node.size()) * node.inverse_scale_factor()).max(Vec2::ZERO)
}

fn scroll_by(position: &mut ScrollPosition, node: &ComputedNode, axes: ScrollAxes, delta: Vec2) {
    let target = (position.0 + delta * axes.mask()).clamp(Vec2::ZERO, max_scroll(node));
    if position.0 != target {
        position.0 = target;
    }
}

pub(crate) fn drag_scroll(
    mut router: ResMut<PointerRouter>,
    time: Res<Time<Real>>,
    mut views: Query<(Entity, &mut ScrollView, &mut ScrollPosition, &ComputedNode)>,
) {
    let delta_time = time.delta_secs().max(f32::EPSILON);
    for (entity, mut view, mut position, node) in &mut views {
        if view.candidate.is_none()
            && view.dragging.is_none()
            && let Some(press) = router.press_on(entity)
        {
            view.candidate = Some(press.id);
            view.velocity = Vec2::ZERO;
        }

        if let Some(id) = view.candidate {
            match router.get(id) {
                Some(pointer) if pointer.is_down() && pointer.owner().is_none() => {
                    let travel = (pointer.position() - pointer.press_position()) * view.axes.mask();
                    if travel.length() >= view.drag_threshold && router.capture(id, entity) {
                        view.candidate = None;
                        view.dragging = Some(id);
                    }
                }
                _ => view.candidate = None,
            }
        }

        if let Some(id) = view.dragging {
            match router.get(id) {
                Some(pointer) if pointer.owner() == Some(entity) && pointer.is_down() => {
                    let delta = -pointer.delta();
                    scroll_by(&mut position, node, view.axes, delta);
                    let instant = delta / delta_time;
                    view.velocity = view.velocity.lerp(instant * view.axes.mask(), 0.4);
                }
                Some(pointer) if pointer.owner() == Some(entity) && pointer.has_ended() => {
                    view.dragging = None;
                }
                _ => {
                    view.dragging = None;
                    view.velocity = Vec2::ZERO;
                }
            }
            continue;
        }

        if view.velocity.length_squared() > 1.0 {
            let velocity = view.velocity;
            scroll_by(&mut position, node, view.axes, velocity * delta_time);
            let decay = view.friction.powf(delta_time);
            view.velocity *= decay;
        } else if view.velocity != Vec2::ZERO {
            view.velocity = Vec2::ZERO;
        }
    }
}

pub(crate) fn wheel_scroll(
    wheel: Option<Res<AccumulatedMouseScroll>>,
    router: Res<PointerRouter>,
    mut views: Query<(&mut ScrollView, &mut ScrollPosition, &ComputedNode)>,
) {
    let Some(wheel) = wheel else {
        return;
    };
    if wheel.delta == Vec2::ZERO {
        return;
    }
    let Some(mouse) = router.get(PointerId::Mouse) else {
        return;
    };
    // The innermost view under the cursor scrolls: hover targets list children first.
    let target = mouse
        .hover_targets()
        .iter()
        .find(|entity| views.contains(**entity))
        .copied();
    let Some(target) = target else {
        return;
    };
    let Ok((mut view, mut position, node)) = views.get_mut(target) else {
        return;
    };
    let mut delta = -wheel.delta;
    if wheel.unit == MouseScrollUnit::Line {
        delta *= view.line_height;
    }
    if view.axes == ScrollAxes::Horizontal && delta.x == 0.0 {
        delta = Vec2::new(delta.y, 0.0);
    }
    view.velocity = Vec2::ZERO;
    let axes = view.axes;
    scroll_by(&mut position, node, axes, delta);
}

pub(crate) fn remember_scroll(
    removed: On<Remove, ScrollMemoryKey>,
    views: Query<(&ScrollMemoryKey, &ScrollPosition)>,
    mut memory: ResMut<ScrollMemory>,
) {
    if let Ok((key, position)) = views.get(removed.entity) {
        memory.positions.insert(key.0, position.0);
    }
}

pub(crate) fn schedule_scroll_restore(
    added: On<Add, ScrollMemoryKey>,
    keys: Query<&ScrollMemoryKey>,
    memory: Res<ScrollMemory>,
    mut commands: Commands,
) {
    if let Ok(key) = keys.get(added.entity)
        && let Some(position) = memory.get(key.0)
    {
        commands
            .entity(added.entity)
            .insert(PendingScrollRestore(position));
    }
}

pub(crate) fn apply_scroll_restore(
    mut views: Query<(Entity, &PendingScrollRestore, &mut ScrollPosition, &ComputedNode)>,
    mut commands: Commands,
) {
    for (entity, pending, mut position, node) in &mut views {
        let limit = max_scroll(node);
        if limit.cmpge(pending.0).all() || node.content_size() != Vec2::ZERO {
            position.0 = pending.0.min(limit);
            commands.entity(entity).remove::<PendingScrollRestore>();
        }
    }
}

/// Keeps the focused widget visible by scrolling its nearest scroll view.
pub(crate) fn scroll_focus_into_view(
    focus: Res<bevy::input_focus::InputFocus>,
    nodes: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform)>,
    mut views: Query<(&ScrollView, &mut ScrollPosition, &ComputedNode, &bevy::ui::UiGlobalTransform)>,
    parents: Query<&ChildOf>,
) {
    if !focus.is_changed() {
        return;
    }
    let Some(focused) = focus.get() else {
        return;
    };
    let Ok((node, transform)) = nodes.get(focused) else {
        return;
    };
    let target = crate::geometry::logical_rect(node, transform);
    let mut current = parents.get(focused).ok().map(ChildOf::parent);
    while let Some(entity) = current {
        if let Ok((view, mut position, view_node, view_transform)) = views.get_mut(entity) {
            let viewport = crate::geometry::logical_rect(view_node, view_transform);
            let mut delta = Vec2::ZERO;
            if target.min.y < viewport.min.y {
                delta.y = target.min.y - viewport.min.y;
            } else if target.max.y > viewport.max.y {
                delta.y = target.max.y - viewport.max.y;
            }
            if target.min.x < viewport.min.x {
                delta.x = target.min.x - viewport.min.x;
            } else if target.max.x > viewport.max.x {
                delta.x = target.max.x - viewport.max.x;
            }
            scroll_by(&mut position, view_node, view.axes, delta);
            return;
        }
        current = parents.get(entity).ok().map(ChildOf::parent);
    }
}
