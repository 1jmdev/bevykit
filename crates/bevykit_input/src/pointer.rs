//! A unified pointer model for mouse and touch, with single-owner capture.
//!
//! Every mouse cursor and touch contact is a pointer with an identity, a position, and a
//! lifecycle: it is pressed, held, and finally released or cancelled. An interaction such as a
//! drag, a scroll view, or a joystick can *capture* a pointer. A captured pointer has exactly
//! one owner, and release or cancellation always ends the capture.
//!
//! When another owner takes a pointer, or the pointer ends, the previous owner receives
//! [`PointerCaptureLost`], so a button can cancel its press when a scroll view takes over.
//!
//! ```ignore
//! fn begin_drag(mut pointers: ResMut<PointerRouter>, targets: Query<Entity, With<Draggable>>) {
//!     for entity in &targets {
//!         if let Some(press) = pointers.press_on(entity) {
//!             pointers.capture(press.id, entity);
//!         }
//!     }
//! }
//! ```

use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::Node;
use bevy::window::PrimaryWindow;
use smallvec::SmallVec;

use crate::InputSuspension;

/// The lifecycle phase of a pointer during the current frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum PointerPhase {
    /// The pointer is over the window without being pressed. Only the mouse hovers.
    Hovering,
    /// The pointer was pressed this frame.
    Pressed,
    /// The pointer is held.
    Held,
    /// The pointer was released this frame.
    Released,
    /// The pointer was cancelled this frame, for example by the system or by suspension.
    Cancelled,
}

/// The state of one pointer.
#[derive(Clone, Debug)]
pub struct Pointer {
    id: PointerId,
    phase: PointerPhase,
    position: Vec2,
    previous_position: Vec2,
    press_position: Vec2,
    press_time: f64,
    owner: Option<Entity>,
    hover_targets: SmallVec<[Entity; 8]>,
    press_targets: SmallVec<[Entity; 8]>,
    over_ui: bool,
    pressed_over_ui: bool,
}

impl Pointer {
    fn new(id: PointerId, position: Vec2) -> Self {
        Self {
            id,
            phase: PointerPhase::Hovering,
            position,
            previous_position: position,
            press_position: position,
            press_time: 0.0,
            owner: None,
            hover_targets: SmallVec::new(),
            press_targets: SmallVec::new(),
            over_ui: false,
            pressed_over_ui: false,
        }
    }

    /// Returns the pointer's identity.
    pub fn id(&self) -> PointerId {
        self.id
    }

    /// Returns the lifecycle phase for this frame.
    pub fn phase(&self) -> PointerPhase {
        self.phase
    }

    /// Returns `true` while the pointer is pressed or held.
    pub fn is_down(&self) -> bool {
        matches!(self.phase, PointerPhase::Pressed | PointerPhase::Held)
    }

    /// Returns `true` on the frame the pointer was released or cancelled.
    pub fn has_ended(&self) -> bool {
        matches!(self.phase, PointerPhase::Released | PointerPhase::Cancelled)
    }

    /// Returns the position in logical window pixels, origin at the top-left corner.
    pub fn position(&self) -> Vec2 {
        self.position
    }

    /// Returns the movement since the previous frame.
    pub fn delta(&self) -> Vec2 {
        self.position - self.previous_position
    }

    /// Returns the position where the current press began.
    pub fn press_position(&self) -> Vec2 {
        self.press_position
    }

    /// Returns the distance travelled from the press position.
    pub fn travel(&self) -> f32 {
        self.position.distance(self.press_position)
    }

    /// Returns the time, in real seconds since startup, when the current press began.
    pub fn press_time(&self) -> f64 {
        self.press_time
    }

    /// Returns the entity that owns the pointer, if it is captured.
    pub fn owner(&self) -> Option<Entity> {
        self.owner
    }

    /// Returns the entities under the pointer when it was pressed, including their ancestors.
    pub fn press_targets(&self) -> &[Entity] {
        &self.press_targets
    }

    /// Returns the entities currently under the pointer, including their ancestors.
    pub fn hover_targets(&self) -> &[Entity] {
        &self.hover_targets
    }

    /// Returns `true` if the pointer is currently over `entity` or one of its descendants.
    pub fn is_over(&self, entity: Entity) -> bool {
        self.hover_targets.contains(&entity)
    }

    /// Returns `true` if the pointer is currently over a UI node.
    pub fn over_ui(&self) -> bool {
        self.over_ui
    }

    /// Returns `true` if the press began over a UI node.
    pub fn pressed_over_ui(&self) -> bool {
        self.pressed_over_ui
    }
}

/// A pointer pressed on a specific entity this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPress {
    /// The pressed pointer.
    pub id: PointerId,
    /// Where the press happened.
    pub position: Vec2,
}

/// Why a capture ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum CaptureEnd {
    /// The pointer was released.
    Released,
    /// The pointer was cancelled.
    Cancelled,
    /// Another entity captured the pointer.
    Taken,
    /// The owner released the capture voluntarily.
    Relinquished,
}

/// Triggered on an entity when it stops owning a pointer.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct PointerCaptureLost {
    /// The former owner.
    pub entity: Entity,
    /// The pointer.
    pub pointer: PointerId,
    /// Why the capture ended.
    pub reason: CaptureEnd,
}

/// Tracks every pointer and arbitrates ownership.
#[derive(Resource, Default, Debug)]
pub struct PointerRouter {
    pointers: Vec<Pointer>,
    lost: Vec<PointerCaptureLost>,
}

impl PointerRouter {
    /// Iterates every known pointer.
    pub fn pointers(&self) -> impl Iterator<Item = &Pointer> {
        self.pointers.iter()
    }

    /// Returns a pointer by identity.
    pub fn get(&self, id: PointerId) -> Option<&Pointer> {
        self.pointers.iter().find(|pointer| pointer.id == id)
    }

    /// Returns a pointer pressed this frame on `entity` or one of its descendants that no other
    /// entity has captured.
    pub fn press_on(&self, entity: Entity) -> Option<PointerPress> {
        self.pointers
            .iter()
            .find(|pointer| {
                pointer.phase == PointerPhase::Pressed
                    && pointer.owner.is_none_or(|owner| owner == entity)
                    && pointer.press_targets.contains(&entity)
            })
            .map(|pointer| PointerPress {
                id: pointer.id,
                position: pointer.position,
            })
    }

    /// Captures a pointer for `entity`. Returns `false` if the pointer is not down.
    ///
    /// Capturing a pointer owned by another entity takes it; the previous owner receives
    /// [`PointerCaptureLost`] with [`CaptureEnd::Taken`].
    pub fn capture(&mut self, id: PointerId, entity: Entity) -> bool {
        let Some(pointer) = self.pointers.iter_mut().find(|pointer| pointer.id == id) else {
            return false;
        };
        if !pointer.is_down() {
            return false;
        }
        if let Some(previous) = pointer.owner.replace(entity)
            && previous != entity
        {
            self.lost.push(PointerCaptureLost {
                entity: previous,
                pointer: id,
                reason: CaptureEnd::Taken,
            });
        }
        true
    }

    /// Releases a capture held by `entity`.
    pub fn release(&mut self, id: PointerId, entity: Entity) {
        if let Some(pointer) = self
            .pointers
            .iter_mut()
            .find(|pointer| pointer.id == id && pointer.owner == Some(entity))
        {
            pointer.owner = None;
            self.lost.push(PointerCaptureLost {
                entity,
                pointer: id,
                reason: CaptureEnd::Relinquished,
            });
        }
    }

    /// Returns the owner of a pointer.
    pub fn owner(&self, id: PointerId) -> Option<Entity> {
        self.get(id).and_then(|pointer| pointer.owner)
    }

    /// Iterates the pointers captured by `entity`, including ones that ended this frame.
    pub fn captured_by(&self, entity: Entity) -> impl Iterator<Item = &Pointer> {
        self.pointers
            .iter()
            .filter(move |pointer| pointer.owner == Some(entity))
    }

    /// Returns `true` if any pointer that is down began over a UI node.
    pub fn any_down_over_ui(&self) -> bool {
        self.pointers
            .iter()
            .any(|pointer| pointer.is_down() && pointer.pressed_over_ui)
    }

    /// Cancels every pointer that is down.
    pub fn cancel_all(&mut self) {
        for pointer in &mut self.pointers {
            if pointer.is_down() {
                pointer.phase = PointerPhase::Cancelled;
            }
        }
    }

    fn pointer_mut(&mut self, id: PointerId, position: Vec2) -> &mut Pointer {
        let index = match self.pointers.iter().position(|pointer| pointer.id == id) {
            Some(index) => index,
            None => {
                self.pointers.push(Pointer::new(id, position));
                self.pointers.len() - 1
            }
        };
        &mut self.pointers[index]
    }

    /// Advances pointers that ended last frame: touches are removed and the mouse hovers.
    fn retire_ended(&mut self) {
        for pointer in &mut self.pointers {
            pointer.previous_position = pointer.position;
            if pointer.has_ended() {
                pointer.phase = PointerPhase::Hovering;
                pointer.owner = None;
                pointer.press_targets.clear();
                pointer.pressed_over_ui = false;
            } else if pointer.phase == PointerPhase::Pressed {
                pointer.phase = PointerPhase::Held;
            }
        }
        self.pointers.retain(|pointer| {
            !matches!(pointer.id, PointerId::Touch(_)) || pointer.phase != PointerPhase::Hovering
        });
    }
}

/// Samples mouse and touch input into the [`PointerRouter`].
pub(crate) fn update_pointers(
    mut router: ResMut<PointerRouter>,
    mut commands: Commands,
    time: Res<Time<Real>>,
    suspension: Res<InputSuspension>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    touches: Option<Res<Touches>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    hover_map: Option<Res<HoverMap>>,
    parents: Query<&ChildOf>,
    nodes: Query<(), With<Node>>,
) {
    router.retire_ended();
    let now = time.elapsed_secs_f64();

    if let Ok(window) = windows.single() {
        let cursor = window.cursor_position();
        match (cursor, mouse.as_deref()) {
            (Some(position), Some(mouse)) => {
                let pointer = router.pointer_mut(PointerId::Mouse, position);
                pointer.position = position;
                if mouse.just_pressed(MouseButton::Left) && !pointer.is_down() {
                    pointer.phase = PointerPhase::Pressed;
                } else if mouse.just_released(MouseButton::Left) && pointer.is_down() {
                    pointer.phase = PointerPhase::Released;
                }
            }
            (Some(position), None) => {
                router.pointer_mut(PointerId::Mouse, position).position = position;
            }
            (None, _) => {
                if let Some(pointer) = router
                    .pointers
                    .iter_mut()
                    .find(|pointer| pointer.id == PointerId::Mouse)
                {
                    if pointer.is_down() {
                        let released = mouse
                            .as_deref()
                            .is_none_or(|mouse| !mouse.pressed(MouseButton::Left));
                        if released {
                            pointer.phase = PointerPhase::Released;
                        }
                    } else if !pointer.has_ended() {
                        router.pointers.retain(|pointer| pointer.id != PointerId::Mouse);
                    }
                }
            }
        }
    }

    if let Some(touches) = touches.as_deref() {
        for touch in touches.iter_just_pressed() {
            let pointer = router.pointer_mut(PointerId::Touch(touch.id()), touch.position());
            pointer.position = touch.position();
            pointer.previous_position = touch.position();
            pointer.phase = PointerPhase::Pressed;
        }
        for touch in touches.iter() {
            if touches.just_pressed(touch.id()) {
                continue;
            }
            let pointer = router.pointer_mut(PointerId::Touch(touch.id()), touch.position());
            pointer.position = touch.position();
            if pointer.phase == PointerPhase::Hovering {
                pointer.phase = PointerPhase::Pressed;
            }
        }
        for touch in touches.iter_just_released() {
            let pointer = router.pointer_mut(PointerId::Touch(touch.id()), touch.position());
            pointer.position = touch.position();
            pointer.phase = PointerPhase::Released;
        }
        for touch in touches.iter_just_canceled() {
            let pointer = router.pointer_mut(PointerId::Touch(touch.id()), touch.position());
            pointer.position = touch.position();
            pointer.phase = PointerPhase::Cancelled;
        }
    }

    if suspension.is_suspended() {
        router.cancel_all();
    }

    for pointer in &mut router.pointers {
        pointer.hover_targets.clear();
        pointer.over_ui = false;
        if let Some(hits) = hover_map.as_deref().and_then(|map| map.get(&pointer.id)) {
            for &hit in hits.keys() {
                pointer.over_ui |= nodes.contains(hit);
                let mut current = Some(hit);
                while let Some(entity) = current {
                    if pointer.hover_targets.contains(&entity) {
                        break;
                    }
                    pointer.hover_targets.push(entity);
                    current = parents.get(entity).ok().map(ChildOf::parent);
                }
            }
        }
        if pointer.phase == PointerPhase::Pressed {
            pointer.press_position = pointer.position;
            pointer.press_time = now;
            pointer.owner = None;
            pointer.press_targets = pointer.hover_targets.clone();
            pointer.pressed_over_ui = pointer.over_ui;
        }
    }

    let mut lost = std::mem::take(&mut router.lost);
    for pointer in &router.pointers {
        if let Some(owner) = pointer.owner {
            let reason = match pointer.phase {
                PointerPhase::Released => Some(CaptureEnd::Released),
                PointerPhase::Cancelled => Some(CaptureEnd::Cancelled),
                _ => None,
            };
            if let Some(reason) = reason {
                lost.push(PointerCaptureLost {
                    entity: owner,
                    pointer: pointer.id,
                    reason,
                });
            }
        }
    }
    for event in lost {
        commands.trigger(event);
    }
}

/// Delivers capture changes requested by interaction systems during the frame.
pub(crate) fn flush_capture_events(mut router: ResMut<PointerRouter>, mut commands: Commands) {
    for event in router.lost.drain(..) {
        commands.trigger(event);
    }
}
