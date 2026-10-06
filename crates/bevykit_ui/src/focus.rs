//! Keyboard and controller focus navigation, with focus traps for modal panels.
//!
//! Focus is stored in Bevy's [`InputFocus`] resource, so Bevy's own widgets (such as editable
//! text) and bevykit widgets agree on which entity is focused. Navigation is spatial by default:
//! a direction moves focus to the nearest [`Focusable`] widget in that direction. Explicit
//! [`FocusLinks`] override it where a layout needs a specific order.
//!
//! A [`FocusTrap`] confines navigation to its descendants. When a trap appears, focus moves to
//! its [`InitialFocus`] widget (or its first focusable widget); when it disappears, the focus it
//! replaced is restored.

use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevykit_core::define_key;
use bevykit_input::prelude::*;

use crate::actions::NavigationRequest;
use crate::geometry::logical_rect;
use crate::state::WidgetState;

define_key!(
    /// Names a focusable widget for explicit links and programmatic focus.
    FocusId
);

/// Makes a widget reachable by focus navigation.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(WidgetState)]
pub struct Focusable {
    /// The widget's name, if it is linked to or focused by name.
    pub id: Option<FocusId>,
}

/// Focuses the widget when its focus trap (or the screen, without a trap) appears.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct InitialFocus;

/// Explicit navigation targets, overriding spatial navigation per direction.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct FocusLinks {
    /// Target when navigating up.
    pub up: Option<FocusId>,
    /// Target when navigating down.
    pub down: Option<FocusId>,
    /// Target when navigating left.
    pub left: Option<FocusId>,
    /// Target when navigating right.
    pub right: Option<FocusId>,
}

/// Confines focus navigation to this entity's descendants while it exists.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct FocusTrap;

/// Active focus traps, innermost last, with the focus each one replaced.
#[derive(Resource, Default, Debug)]
pub struct FocusTraps {
    stack: Vec<(Entity, Option<Entity>)>,
}

impl FocusTraps {
    /// Returns the innermost trap.
    pub fn active(&self) -> Option<Entity> {
        self.stack.last().map(|(trap, _)| *trap)
    }
}

/// System parameter for moving focus programmatically.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Focus<'w, 's> {
    focus: ResMut<'w, InputFocus>,
    visible: ResMut<'w, InputFocusVisible>,
    focusables: Query<'w, 's, (Entity, &'static Focusable)>,
}

impl Focus<'_, '_> {
    /// Returns the focused entity.
    pub fn get(&self) -> Option<Entity> {
        self.focus.get()
    }

    /// Focuses an entity.
    pub fn set(&mut self, entity: Entity) {
        self.focus.set(entity, FocusCause::Navigated);
        self.visible.0 = true;
    }

    /// Focuses the widget with the given identifier. Returns `false` if none exists.
    pub fn set_id(&mut self, id: FocusId) -> bool {
        let found = self
            .focusables
            .iter()
            .find(|(_, focusable)| focusable.id == Some(id))
            .map(|(entity, _)| entity);
        if let Some(entity) = found {
            self.set(entity);
        }
        found.is_some()
    }

    /// Removes focus.
    pub fn clear(&mut self) {
        self.focus.clear();
    }
}

pub(crate) fn on_trap_added(
    added: On<Add, FocusTrap>,
    mut traps: ResMut<FocusTraps>,
    focus: Res<InputFocus>,
) {
    traps.stack.push((added.entity, focus.get()));
}

pub(crate) fn on_trap_removed(
    removed: On<Remove, FocusTrap>,
    mut traps: ResMut<FocusTraps>,
    mut focus: ResMut<InputFocus>,
    entities: Query<(), With<Focusable>>,
) {
    let Some(index) = traps.stack.iter().position(|(trap, _)| *trap == removed.entity) else {
        return;
    };
    let (_, previous) = traps.stack.remove(index);
    let is_innermost = index == traps.stack.len();
    if is_innermost {
        match previous.filter(|entity| entities.contains(*entity)) {
            Some(previous) => focus.set(previous, FocusCause::Navigated),
            None => focus.clear(),
        }
    }
}

fn is_inside(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = Some(entity);
    while let Some(candidate) = current {
        if candidate == ancestor {
            return true;
        }
        current = parents.get(candidate).ok().map(ChildOf::parent);
    }
    false
}

/// Moves focus into a newly active trap, or to an initial focus target outside any trap.
pub(crate) fn apply_initial_focus(
    traps: Res<FocusTraps>,
    mut focus: ResMut<InputFocus>,
    initial: Query<Entity, (With<InitialFocus>, With<Focusable>)>,
    focusables: Query<(Entity, &WidgetState, &ComputedNode, &UiGlobalTransform), With<Focusable>>,
    added_initial: Query<Entity, Added<InitialFocus>>,
    parents: Query<&ChildOf>,
    mut settled_trap: Local<Option<Entity>>,
) {
    let trap = traps.active();
    let inside = |entity: Entity| trap.is_none_or(|trap| is_inside(entity, trap, &parents));
    let focused_inside = focus
        .get()
        .is_some_and(|entity| focusables.contains(entity) && inside(entity));

    let trap_changed = *settled_trap != trap;
    if !trap_changed && (focused_inside || added_initial.is_empty()) {
        return;
    }

    let target = initial.iter().find(|entity| inside(*entity)).or_else(|| {
        trap?;
        // Without an explicit initial focus, pick the top-left focusable widget in the trap.
        focusables
            .iter()
            .filter(|(entity, state, node, _)| {
                inside(*entity) && !state.disabled && node.size() != Vec2::ZERO
            })
            .min_by(|a, b| {
                let a = logical_rect(a.2, a.3).min;
                let b = logical_rect(b.2, b.3).min;
                (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(entity, ..)| entity)
    });
    if let Some(target) = target {
        focus.set(target, FocusCause::Navigated);
        *settled_trap = trap;
    } else if trap.is_none() {
        *settled_trap = None;
    }
}

/// Moves focus in the requested direction.
pub(crate) fn navigate(
    request: Res<NavigationRequest>,
    traps: Res<FocusTraps>,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
    focusables: Query<(
        Entity,
        &Focusable,
        &WidgetState,
        &ComputedNode,
        &UiGlobalTransform,
        &InheritedVisibility,
        Option<&FocusLinks>,
    )>,
    parents: Query<&ChildOf>,
) {
    let Some(direction) = request.0 else {
        return;
    };
    if !visible.0 {
        visible.0 = true;
    }
    let trap = traps.active();
    let candidates: Vec<(Entity, Option<FocusId>, Rect)> = focusables
        .iter()
        .filter(|(entity, _, state, node, _, visibility, _)| {
            !state.disabled
                && visibility.get()
                && node.size() != Vec2::ZERO
                && trap.is_none_or(|trap| is_inside(*entity, trap, &parents))
        })
        .map(|(entity, focusable, _, node, transform, ..)| {
            (entity, focusable.id, logical_rect(node, transform))
        })
        .collect();

    let current = focus
        .get()
        .and_then(|entity| candidates.iter().find(|(candidate, ..)| *candidate == entity));
    let Some(&(current_entity, _, current_rect)) = current else {
        // Nothing focused yet: start from the top-left widget.
        if let Some((entity, ..)) = candidates.iter().min_by(|a, b| {
            (a.2.min.y, a.2.min.x)
                .partial_cmp(&(b.2.min.y, b.2.min.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            focus.set(*entity, FocusCause::Navigated);
        }
        return;
    };

    // Screen space has Y pointing down; navigation directions have Y pointing up.
    let screen_direction = Vec2::new(direction.x, -direction.y);

    if let Ok((.., Some(links))) = focusables.get(current_entity) {
        let link = if screen_direction.y < 0.0 {
            links.up
        } else if screen_direction.y > 0.0 {
            links.down
        } else if screen_direction.x < 0.0 {
            links.left
        } else {
            links.right
        };
        if let Some(link) = link
            && let Some((entity, ..)) = candidates.iter().find(|(_, id, _)| *id == Some(link))
        {
            focus.set(*entity, FocusCause::Navigated);
            return;
        }
    }

    let origin = current_rect.center();
    let best = candidates
        .iter()
        .filter(|(entity, ..)| *entity != current_entity)
        .filter_map(|(entity, _, rect)| {
            let offset = rect.center() - origin;
            let along = offset.dot(screen_direction);
            if along <= 1.0 {
                return None;
            }
            let across = (offset - screen_direction * along).length();
            // Prefer widgets straight ahead: sideways distance costs more than forward distance.
            Some((*entity, along + across * 2.0))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    if let Some((entity, _)) = best {
        focus.set(entity, FocusCause::Navigated);
    }
}

/// Focuses focusable widgets pressed with a pointer and hides the focus outline.
pub(crate) fn focus_on_press(
    router: Res<PointerRouter>,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
    focusables: Query<&WidgetState, With<Focusable>>,
) {
    for pointer in router.pointers() {
        if pointer.phase() != PointerPhase::Pressed {
            continue;
        }
        if visible.0 {
            visible.0 = false;
        }
        let pressed = pointer
            .press_targets()
            .iter()
            .find(|entity| focusables.get(**entity).is_ok_and(|state| !state.disabled));
        if let Some(entity) = pressed
            && focus.get() != Some(*entity)
        {
            focus.set(*entity, FocusCause::Pressed);
        }
    }
}

/// Mirrors [`InputFocus`] into [`WidgetState::focused`].
pub(crate) fn sync_focused_state(
    focus: Res<InputFocus>,
    mut widgets: Query<(Entity, &mut WidgetState)>,
    mut previous: Local<Option<Entity>>,
) {
    let current = focus.get();
    if *previous == current && !focus.is_changed() {
        return;
    }
    if let Some(old) = *previous
        && let Ok((_, mut state)) = widgets.get_mut(old)
    {
        state.focused = false;
    }
    if let Some(new) = current
        && let Ok((_, mut state)) = widgets.get_mut(new)
    {
        state.focused = true;
    }
    *previous = current;
}
