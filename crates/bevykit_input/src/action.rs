//! Named actions and their per-frame state.

use core::fmt::Debug;
use core::hash::Hash;
use core::time::Duration;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// A game-defined input action. Usually implemented with `#[derive(KitAction)]`.
///
/// Every action carries a button state, a scalar value, and a two-dimensional axis, so the
/// same action can be driven by a key, a trigger, or a stick. Bindings decide which parts are
/// written; game code reads whichever form suits it.
pub trait Action: Copy + Eq + Hash + Debug + Send + Sync + 'static {
    /// Every variant of the action type, in declaration order.
    fn variants() -> &'static [Self];

    /// A stable name used for saved bindings and diagnostics.
    fn name(&self) -> &'static str;

    /// Finds the action with the given name.
    fn from_name(name: &str) -> Option<Self> {
        Self::variants()
            .iter()
            .copied()
            .find(|action| action.name() == name)
    }
}

/// The state of one action during the current frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct ActionData {
    /// Whether the action is held.
    pub pressed: bool,
    /// Whether the action was held during the previous update.
    pub was_pressed: bool,
    /// The scalar value, in `0.0..=1.0` for buttons and `-1.0..=1.0` for axes.
    pub value: f32,
    /// The two-dimensional value, with each component in `-1.0..=1.0`.
    pub axis: Vec2,
    /// How long the action has been held continuously.
    pub held: Duration,
    /// Whether a consumer has claimed this action for the current frame.
    pub consumed: bool,
}

impl ActionData {
    /// Returns `true` on the update the action became pressed.
    pub fn just_pressed(&self) -> bool {
        self.pressed && !self.was_pressed
    }

    /// Returns `true` on the update the action stopped being pressed.
    pub fn just_released(&self) -> bool {
        !self.pressed && self.was_pressed
    }
}

/// The state of every action of type `A`.
///
/// Updated once per frame in `PreUpdate`. Actions belonging to inactive or blocked contexts
/// read as released and zero.
#[derive(Resource, Debug)]
pub struct ActionState<A: Action> {
    actions: HashMap<A, ActionData>,
}

impl<A: Action> Default for ActionState<A> {
    fn default() -> Self {
        let actions = A::variants()
            .iter()
            .map(|action| (*action, ActionData::default()))
            .collect();
        Self { actions }
    }
}

impl<A: Action> ActionState<A> {
    /// Returns the full state of an action.
    pub fn data(&self, action: A) -> ActionData {
        self.actions
            .get(&action)
            .filter(|data| !data.consumed)
            .copied()
            .unwrap_or_default()
    }

    /// Returns `true` while the action is held.
    pub fn pressed(&self, action: A) -> bool {
        self.data(action).pressed
    }

    /// Returns `true` on the frame the action became pressed.
    pub fn just_pressed(&self, action: A) -> bool {
        self.data(action).just_pressed()
    }

    /// Returns `true` on the frame the action was released.
    pub fn just_released(&self, action: A) -> bool {
        self.data(action).just_released()
    }

    /// Returns the scalar value of the action.
    pub fn value(&self, action: A) -> f32 {
        self.data(action).value
    }

    /// Returns the two-dimensional value of the action.
    pub fn axis2(&self, action: A) -> Vec2 {
        self.data(action).axis
    }

    /// Returns how long the action has been held.
    pub fn held_for(&self, action: A) -> Duration {
        self.data(action).held
    }

    /// Claims the action for the rest of the frame, so later readers see it as released.
    ///
    /// Useful when a UI element handles an action that gameplay would otherwise also react to.
    pub fn consume(&mut self, action: A) {
        if let Some(data) = self.actions.get_mut(&action) {
            data.consumed = true;
        }
    }

    /// Releases every action and forgets held durations.
    pub fn reset(&mut self) {
        for data in self.actions.values_mut() {
            *data = ActionData {
                was_pressed: data.pressed,
                ..default()
            };
        }
    }

    /// Iterates every action with its state.
    pub fn iter(&self) -> impl Iterator<Item = (A, &ActionData)> {
        self.actions.iter().map(|(action, data)| (*action, data))
    }

    pub(crate) fn begin_update(&mut self) {
        for data in self.actions.values_mut() {
            data.was_pressed = data.pressed;
            data.pressed = false;
            data.value = 0.0;
            data.axis = Vec2::ZERO;
            data.consumed = false;
        }
    }

    pub(crate) fn data_mut(&mut self, action: A) -> &mut ActionData {
        self.actions.entry(action).or_default()
    }

    pub(crate) fn finish_update(&mut self, delta: Duration) {
        for data in self.actions.values_mut() {
            if data.pressed {
                data.held = if data.was_pressed {
                    data.held + delta
                } else {
                    Duration::ZERO
                };
            } else {
                data.held = Duration::ZERO;
            }
        }
    }
}
