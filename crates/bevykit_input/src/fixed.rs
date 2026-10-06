//! Action state for fixed-timestep simulation.
//!
//! Frames and simulation ticks do not line up: a frame may run no tick, or several. Reading
//! [`ActionState`](crate::action::ActionState) from `FixedUpdate` would lose presses on frames
//! without a tick and repeat them on frames with several.
//!
//! [`FixedActionState`] fixes both. Button edges are queued when they happen and delivered to
//! the next tick exactly once. Held values (pressed, scalar, and axis) reflect the most recent
//! frame on every tick. A press shorter than a frame still reads as pressed on the tick that
//! receives its edge.
//!
//! ```ignore
//! fn simulate(actions: Res<FixedActionState<GameAction>>) {
//!     if actions.just_pressed(GameAction::Jump) {
//!         // Runs exactly once per press, regardless of frame rate.
//!     }
//! }
//! ```

use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use crate::action::{Action, ActionData, ActionState};

/// Action state for use in `FixedUpdate`.
#[derive(Resource, Debug)]
pub struct FixedActionState<A: Action> {
    current: Vec<(A, ActionData)>,
    just_pressed: HashSet<A>,
    just_released: HashSet<A>,
    pending_pressed: HashSet<A>,
    pending_released: HashSet<A>,
}

impl<A: Action> Default for FixedActionState<A> {
    fn default() -> Self {
        Self {
            current: Vec::new(),
            just_pressed: HashSet::default(),
            just_released: HashSet::default(),
            pending_pressed: HashSet::default(),
            pending_released: HashSet::default(),
        }
    }
}

impl<A: Action> FixedActionState<A> {
    fn data(&self, action: A) -> ActionData {
        self.current
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, data)| *data)
            .unwrap_or_default()
    }

    /// Returns `true` while the action is held, or if it was pressed since the last tick.
    pub fn pressed(&self, action: A) -> bool {
        self.data(action).pressed || self.just_pressed.contains(&action)
    }

    /// Returns `true` on the first tick after the action was pressed.
    pub fn just_pressed(&self, action: A) -> bool {
        self.just_pressed.contains(&action)
    }

    /// Returns `true` on the first tick after the action was released.
    pub fn just_released(&self, action: A) -> bool {
        self.just_released.contains(&action)
    }

    /// Returns the most recent scalar value.
    pub fn value(&self, action: A) -> f32 {
        self.data(action).value
    }

    /// Returns the most recent two-dimensional value.
    pub fn axis2(&self, action: A) -> Vec2 {
        self.data(action).axis
    }
}

/// Queues the frame's edges for the next tick. Runs every frame after the action update.
pub(crate) fn queue_fixed_edges<A: Action>(
    frame: Res<ActionState<A>>,
    mut fixed: ResMut<FixedActionState<A>>,
) {
    let fixed = &mut *fixed;
    fixed.current.clear();
    for (action, data) in frame.iter() {
        fixed.current.push((action, *data));
        if data.just_pressed() {
            fixed.pending_pressed.insert(action);
        }
        if data.just_released() {
            fixed.pending_released.insert(action);
        }
    }
}

/// Delivers queued edges to the current tick. Runs at the start of every fixed tick.
pub(crate) fn deliver_fixed_edges<A: Action>(mut fixed: ResMut<FixedActionState<A>>) {
    let fixed = &mut *fixed;
    fixed.just_pressed = std::mem::take(&mut fixed.pending_pressed);
    fixed.just_released = std::mem::take(&mut fixed.pending_released);
}
