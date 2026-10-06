//! Gesture recognition on interaction surfaces.
//!
//! An [`InteractionSurface`] with a [`GesturePolicy`] receives gesture events as entity
//! events, which can be observed with `.observe(...)` or a global observer. All distances are
//! logical pixels and all durations are seconds.
//!
//! ```ignore
//! commands
//!     .spawn((
//!         InteractionSurface::screen(),
//!         GesturePolicy::default().tap().drag_after_distance(8.0).pinch(),
//!     ))
//!     .observe(|drag: On<GestureDrag>, mut camera: Single<&mut Transform, With<Camera>>| {
//!         camera.translation.x -= drag.delta.x * 0.01;
//!     });
//! ```

use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use smallvec::SmallVec;

use crate::pointer::{PointerPhase, PointerRouter};

/// Which pointers an interaction surface receives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum SurfaceArea {
    /// Pointers pressed on the entity or its descendants, as determined by picking.
    #[default]
    Hit,
    /// Every pointer pressed anywhere outside the UI, such as a world camera surface.
    Screen,
}

/// Marks an entity that recognizes gestures.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
#[require(GesturePolicy, GestureTracker)]
pub struct InteractionSurface {
    /// Which pointers the surface receives.
    pub area: SurfaceArea,
}

impl InteractionSurface {
    /// A surface receiving pointers pressed on the entity.
    pub fn hit() -> Self {
        Self {
            area: SurfaceArea::Hit,
        }
    }

    /// A surface receiving pointers pressed anywhere outside the UI.
    pub fn screen() -> Self {
        Self {
            area: SurfaceArea::Screen,
        }
    }
}

/// Configures which gestures a surface recognizes and their thresholds.
///
/// The default policy recognizes nothing; enable gestures with the builder methods.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct GesturePolicy {
    /// Recognize taps.
    pub tap: bool,
    /// Recognize double taps. A double tap is reported after its second tap.
    pub double_tap: bool,
    /// Recognize holds lasting at least this long.
    pub hold: Option<f32>,
    /// Begin dragging after the pointer travels this far.
    pub drag_distance: Option<f32>,
    /// Recognize two-pointer pinches.
    pub pinch: bool,
    /// Recognize two-pointer rotations.
    pub rotate: bool,
    /// Taps and holds are rejected after travelling farther than this.
    pub tap_slop: f32,
    /// Taps are rejected when held longer than this.
    pub tap_duration: f32,
    /// The longest interval between the taps of a double tap.
    pub double_tap_interval: f32,
}

impl Default for GesturePolicy {
    fn default() -> Self {
        Self {
            tap: false,
            double_tap: false,
            hold: None,
            drag_distance: None,
            pinch: false,
            rotate: false,
            tap_slop: 10.0,
            tap_duration: 0.35,
            double_tap_interval: 0.3,
        }
    }
}

impl GesturePolicy {
    /// Enables taps.
    pub fn tap(mut self) -> Self {
        self.tap = true;
        self
    }

    /// Enables double taps, and taps.
    pub fn double_tap(mut self) -> Self {
        self.tap = true;
        self.double_tap = true;
        self
    }

    /// Enables holds of at least `seconds`.
    pub fn hold(mut self, seconds: f32) -> Self {
        self.hold = Some(seconds);
        self
    }

    /// Enables drags that begin after travelling `distance` logical pixels.
    pub fn drag_after_distance(mut self, distance: f32) -> Self {
        self.drag_distance = Some(distance);
        self
    }

    /// Enables pinches.
    pub fn pinch(mut self) -> Self {
        self.pinch = true;
        self
    }

    /// Enables rotations.
    pub fn rotate(mut self) -> Self {
        self.rotate = true;
        self
    }

    /// Sets the tap slop distance.
    pub fn tap_slop(mut self, distance: f32) -> Self {
        self.tap_slop = distance;
        self
    }
}

/// Per-surface recognition state. Inserted automatically.
#[derive(Component, Clone, Debug, Default)]
pub struct GestureTracker {
    pointers: SmallVec<[PointerId; 2]>,
    dragging: bool,
    hold_fired: bool,
    multi_touch: bool,
    last_tap: Option<(f64, Vec2)>,
    previous_span: Option<(f32, f32)>,
}

/// A completed tap.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureTap {
    /// The surface.
    pub entity: Entity,
    /// Where the tap happened.
    pub position: Vec2,
    /// The pointer that tapped.
    pub pointer: PointerId,
}

/// A second tap shortly after a first one at the same place.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureDoubleTap {
    /// The surface.
    pub entity: Entity,
    /// Where the second tap happened.
    pub position: Vec2,
}

/// A press held in place for the configured duration.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureHold {
    /// The surface.
    pub entity: Entity,
    /// Where the press is held.
    pub position: Vec2,
    /// The pointer being held.
    pub pointer: PointerId,
}

/// A drag began.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureDragStart {
    /// The surface.
    pub entity: Entity,
    /// Where the press began.
    pub origin: Vec2,
    /// The current position.
    pub position: Vec2,
}

/// A drag moved.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureDrag {
    /// The surface.
    pub entity: Entity,
    /// The movement since the previous frame.
    pub delta: Vec2,
    /// The current position.
    pub position: Vec2,
}

/// A drag ended by release.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureDragEnd {
    /// The surface.
    pub entity: Entity,
    /// Where the drag ended.
    pub position: Vec2,
}

/// Two pointers moved apart or together.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GesturePinch {
    /// The surface.
    pub entity: Entity,
    /// The ratio of the current span to the previous frame's span.
    pub scale: f32,
    /// The midpoint between the pointers.
    pub center: Vec2,
}

/// Two pointers rotated around each other.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureRotate {
    /// The surface.
    pub entity: Entity,
    /// The rotation since the previous frame, in radians, clockwise on screen.
    pub angle: f32,
    /// The midpoint between the pointers.
    pub center: Vec2,
}

/// An in-progress gesture was cancelled, by the system or because another interaction took
/// its pointer.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct GestureCancelled {
    /// The surface.
    pub entity: Entity,
}

pub(crate) fn recognize_gestures(
    mut router: ResMut<PointerRouter>,
    mut surfaces: Query<(Entity, &InteractionSurface, &GesturePolicy, &mut GestureTracker)>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, surface, policy, mut tracker) in &mut surfaces {
        acquire_pointers(&router, entity, surface, &mut tracker);

        // Drop pointers taken by another interaction, such as a scroll view.
        let before = tracker.pointers.len();
        tracker.pointers.retain(|id| {
            router
                .get(*id)
                .is_some_and(|pointer| pointer.owner().is_none_or(|owner| owner == entity))
        });
        if tracker.pointers.len() < before && (tracker.dragging || before > 1) {
            commands.trigger(GestureCancelled { entity });
            reset(&mut tracker);
            continue;
        }

        match tracker.pointers.len() {
            0 => {}
            1 => single_pointer(&mut router, entity, policy, &mut tracker, now, &mut commands),
            _ => two_pointers(&mut router, entity, policy, &mut tracker, &mut commands),
        }
    }
}

fn acquire_pointers(
    router: &PointerRouter,
    entity: Entity,
    surface: &InteractionSurface,
    tracker: &mut GestureTracker,
) {
    for pointer in router.pointers() {
        if pointer.phase() != PointerPhase::Pressed || pointer.owner().is_some() {
            continue;
        }
        let eligible = match surface.area {
            SurfaceArea::Hit => pointer.press_targets().contains(&entity),
            SurfaceArea::Screen => !pointer.pressed_over_ui(),
        };
        if eligible && !tracker.pointers.contains(&pointer.id()) && tracker.pointers.len() < 2 {
            if tracker.pointers.is_empty() {
                tracker.hold_fired = false;
                tracker.multi_touch = false;
            }
            tracker.pointers.push(pointer.id());
        }
    }
}

fn single_pointer(
    router: &mut PointerRouter,
    entity: Entity,
    policy: &GesturePolicy,
    tracker: &mut GestureTracker,
    now: f64,
    commands: &mut Commands,
) {
    let id = tracker.pointers[0];
    let Some(pointer) = router.get(id).cloned() else {
        reset(tracker);
        return;
    };
    tracker.previous_span = None;

    match pointer.phase() {
        PointerPhase::Cancelled => {
            if tracker.dragging || tracker.hold_fired {
                commands.trigger(GestureCancelled { entity });
            }
            reset(tracker);
        }
        PointerPhase::Released => {
            if tracker.dragging {
                commands.trigger(GestureDragEnd {
                    entity,
                    position: pointer.position(),
                });
            } else if policy.tap
                && !tracker.hold_fired
                && !tracker.multi_touch
                && pointer.travel() <= policy.tap_slop
                && (now - pointer.press_time()) as f32 <= policy.tap_duration
            {
                let position = pointer.position();
                commands.trigger(GestureTap {
                    entity,
                    position,
                    pointer: id,
                });
                let is_double = tracker.last_tap.is_some_and(|(time, last)| {
                    (now - time) as f32 <= policy.double_tap_interval
                        && last.distance(position) <= policy.tap_slop * 2.0
                });
                if policy.double_tap && is_double {
                    commands.trigger(GestureDoubleTap { entity, position });
                    tracker.last_tap = None;
                } else {
                    tracker.last_tap = Some((now, position));
                }
            }
            let last_tap = tracker.last_tap;
            reset(tracker);
            tracker.last_tap = last_tap;
        }
        PointerPhase::Pressed | PointerPhase::Held => {
            if tracker.dragging {
                if pointer.delta() != Vec2::ZERO {
                    commands.trigger(GestureDrag {
                        entity,
                        delta: pointer.delta(),
                        position: pointer.position(),
                    });
                }
                return;
            }
            if let Some(distance) = policy.drag_distance
                && pointer.travel() >= distance
            {
                tracker.dragging = true;
                router.capture(id, entity);
                commands.trigger(GestureDragStart {
                    entity,
                    origin: pointer.press_position(),
                    position: pointer.position(),
                });
                commands.trigger(GestureDrag {
                    entity,
                    delta: pointer.position() - pointer.press_position(),
                    position: pointer.position(),
                });
                return;
            }
            if let Some(hold) = policy.hold
                && !tracker.hold_fired
                && pointer.travel() <= policy.tap_slop
                && (now - pointer.press_time()) as f32 >= hold
            {
                tracker.hold_fired = true;
                commands.trigger(GestureHold {
                    entity,
                    position: pointer.position(),
                    pointer: id,
                });
            }
        }
        PointerPhase::Hovering => reset(tracker),
    }
}

fn two_pointers(
    router: &mut PointerRouter,
    entity: Entity,
    policy: &GesturePolicy,
    tracker: &mut GestureTracker,
    commands: &mut Commands,
) {
    tracker.multi_touch = true;
    let (Some(first), Some(second)) = (
        router.get(tracker.pointers[0]).cloned(),
        router.get(tracker.pointers[1]).cloned(),
    ) else {
        reset(tracker);
        return;
    };

    if first.phase() == PointerPhase::Cancelled || second.phase() == PointerPhase::Cancelled {
        commands.trigger(GestureCancelled { entity });
        reset(tracker);
        return;
    }

    if tracker.dragging {
        tracker.dragging = false;
        commands.trigger(GestureDragEnd {
            entity,
            position: first.position(),
        });
    }

    if !policy.pinch && !policy.rotate {
        return;
    }
    router.capture(first.id(), entity);
    router.capture(second.id(), entity);

    let offset = second.position() - first.position();
    let span = offset.length();
    let angle = offset.y.atan2(offset.x);
    let center = (first.position() + second.position()) * 0.5;

    if let Some((previous_span, previous_angle)) = tracker.previous_span {
        if policy.pinch && previous_span > f32::EPSILON && span != previous_span {
            commands.trigger(GesturePinch {
                entity,
                scale: span / previous_span,
                center,
            });
        }
        let mut delta = angle - previous_angle;
        if delta > std::f32::consts::PI {
            delta -= std::f32::consts::TAU;
        } else if delta < -std::f32::consts::PI {
            delta += std::f32::consts::TAU;
        }
        if policy.rotate && delta != 0.0 {
            commands.trigger(GestureRotate {
                entity,
                angle: delta,
                center,
            });
        }
    }
    tracker.previous_span = Some((span, angle));

    // When one pointer lifts, continue with the remaining one without tapping.
    if first.has_ended() || second.has_ended() {
        tracker.previous_span = None;
        tracker.pointers.retain(|id| {
            router
                .get(*id)
                .is_some_and(|pointer| !pointer.has_ended())
        });
    }
}

fn reset(tracker: &mut GestureTracker) {
    *tracker = GestureTracker {
        last_tap: tracker.last_tap,
        ..default()
    };
}
