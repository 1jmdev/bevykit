//! Binding tables that map physical inputs to actions, per context.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::binding::{Axis2Binding, AxisBinding, Binding, ButtonBinding};
use crate::context::InputContext;

/// A radial dead zone for thumbsticks.
#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
pub struct DeadZone {
    /// Magnitudes below this value read as zero.
    pub inner: f32,
    /// Magnitudes above this value read as one.
    pub outer: f32,
}

impl Default for DeadZone {
    fn default() -> Self {
        Self {
            inner: 0.15,
            outer: 0.95,
        }
    }
}

impl DeadZone {
    /// Applies the dead zone to a stick value, rescaling the live range to `0.0..=1.0`.
    pub fn apply(&self, value: Vec2) -> Vec2 {
        let length = value.length();
        if length <= self.inner || length == 0.0 {
            return Vec2::ZERO;
        }
        let span = (self.outer - self.inner).max(f32::EPSILON);
        let scaled = ((length - self.inner) / span).min(1.0);
        value / length * scaled
    }

    /// Applies the dead zone to a single axis value.
    pub fn apply_scalar(&self, value: f32) -> f32 {
        self.apply(Vec2::new(value, 0.0)).x
    }
}

/// Selects which gamepads drive a map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum GamepadAssignment {
    /// Every connected gamepad.
    #[default]
    Any,
    /// A single gamepad entity.
    Only(Entity),
    /// No gamepad.
    None,
}

#[derive(Clone, Debug)]
struct ContextBindings<A: Action> {
    context: InputContext,
    bindings: Vec<(A, Binding)>,
}

/// The binding table for actions of type `A`.
#[derive(Resource, Clone, Debug)]
pub struct InputMap<A: Action> {
    contexts: Vec<ContextBindings<A>>,
    defaults: Option<Vec<ContextBindings<A>>>,
    /// Dead zone applied to thumbsticks and gamepad axes.
    pub dead_zone: DeadZone,
    /// Analog buttons count as pressed above this value.
    pub trigger_threshold: f32,
    /// Which gamepads drive this map.
    pub gamepad: GamepadAssignment,
}

impl<A: Action> Default for InputMap<A> {
    fn default() -> Self {
        Self {
            contexts: Vec::new(),
            defaults: None,
            dead_zone: DeadZone::default(),
            trigger_threshold: 0.5,
            gamepad: GamepadAssignment::Any,
        }
    }
}

/// Builder for adding bindings to one context. Returned by [`InputMap::context`].
pub struct ContextBuilder<'a, A: Action> {
    bindings: &'a mut Vec<(A, Binding)>,
}

impl<A: Action> ContextBuilder<'_, A> {
    /// Binds a digital input.
    pub fn button(&mut self, action: A, binding: impl Into<ButtonBinding>) -> &mut Self {
        self.bind(action, Binding::Button(binding.into()))
    }

    /// Binds a one-dimensional input.
    pub fn axis(&mut self, action: A, binding: impl Into<AxisBinding>) -> &mut Self {
        self.bind(action, Binding::Axis(binding.into()))
    }

    /// Binds a two-dimensional input.
    pub fn axis2(&mut self, action: A, binding: impl Into<Axis2Binding>) -> &mut Self {
        self.bind(action, Binding::Axis2(binding.into()))
    }

    /// Binds any input.
    pub fn bind(&mut self, action: A, binding: impl Into<Binding>) -> &mut Self {
        let binding = binding.into();
        if !self
            .bindings
            .iter()
            .any(|(existing_action, existing)| *existing_action == action && *existing == binding)
        {
            self.bindings.push((action, binding));
        }
        self
    }
}

/// Two or more actions in the same context reading the same input.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingConflict<A: Action> {
    /// The context containing the conflict.
    pub context: InputContext,
    /// The shared input.
    pub binding: Binding,
    /// The actions reading it.
    pub actions: Vec<A>,
}

/// A serializable snapshot of a map's bindings, stored by action and context name.
///
/// Virtual bindings are not persisted; they belong to the on-screen layout, not the player.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedBindings {
    /// Bindings per context.
    pub contexts: Vec<SavedContext>,
}

/// Saved bindings for one context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedContext {
    /// The context name.
    pub context: InputContext,
    /// Bindings per action name.
    pub actions: Vec<(String, Vec<Binding>)>,
}

impl<A: Action> InputMap<A> {
    /// Returns a builder for the bindings of a context.
    pub fn context(&mut self, context: impl Into<InputContext>) -> ContextBuilder<'_, A> {
        let index = self.context_index_or_insert(context.into());
        ContextBuilder {
            bindings: &mut self.contexts[index].bindings,
        }
    }

    /// Iterates every binding of every context.
    pub fn iter(&self) -> impl Iterator<Item = (&InputContext, A, &Binding)> {
        self.contexts.iter().flat_map(|context| {
            context
                .bindings
                .iter()
                .map(move |(action, binding)| (&context.context, *action, binding))
        })
    }

    /// Returns the bindings of an action within a context.
    pub fn bindings(&self, context: &InputContext, action: A) -> Vec<Binding> {
        self.contexts
            .iter()
            .filter(|entry| &entry.context == context)
            .flat_map(|entry| entry.bindings.iter())
            .filter(|(bound, _)| *bound == action)
            .map(|(_, binding)| *binding)
            .collect()
    }

    /// Returns human-readable descriptions of an action's bindings in every context.
    pub fn describe(&self, action: A) -> Vec<String> {
        let mut descriptions: Vec<String> = Vec::new();
        for (_, bound, binding) in self.iter() {
            let description = binding.describe();
            if bound == action && !descriptions.contains(&description) {
                descriptions.push(description);
            }
        }
        descriptions
    }

    /// Replaces the binding at `slot` of an action, or appends it when `slot` is past the end.
    ///
    /// The first modification records the current bindings as the defaults restored by
    /// [`reset_to_defaults`](Self::reset_to_defaults).
    pub fn rebind(
        &mut self,
        context: &InputContext,
        action: A,
        slot: usize,
        binding: impl Into<Binding>,
    ) {
        self.capture_defaults();
        let binding = binding.into();
        let index = self.context_index_or_insert(context.clone());
        let bindings = &mut self.contexts[index].bindings;
        let existing: Vec<usize> = bindings
            .iter()
            .enumerate()
            .filter(|(_, (bound, _))| *bound == action)
            .map(|(position, _)| position)
            .collect();
        match existing.get(slot) {
            Some(&position) => bindings[position].1 = binding,
            None => bindings.push((action, binding)),
        }
    }

    /// Removes every binding of an action within a context.
    pub fn clear(&mut self, context: &InputContext, action: A) {
        self.capture_defaults();
        for entry in self.contexts.iter_mut().filter(|entry| &entry.context == context) {
            entry.bindings.retain(|(bound, _)| *bound != action);
        }
    }

    /// Restores the bindings recorded before the first modification.
    pub fn reset_to_defaults(&mut self) {
        if let Some(defaults) = self.defaults.clone() {
            self.contexts = defaults;
        }
    }

    /// Returns every input that drives more than one action within the same context.
    pub fn conflicts(&self) -> Vec<BindingConflict<A>> {
        let mut conflicts = Vec::new();
        for entry in &self.contexts {
            for (index, (action, binding)) in entry.bindings.iter().enumerate() {
                let mut actions = vec![*action];
                for (other_action, other) in &entry.bindings[index + 1..] {
                    if other_action != action
                        && binding.overlaps(other)
                        && !actions.contains(other_action)
                    {
                        actions.push(*other_action);
                    }
                }
                let already_reported = conflicts.iter().any(|conflict: &BindingConflict<A>| {
                    conflict.context == entry.context && conflict.binding.overlaps(binding)
                });
                if actions.len() > 1 && !already_reported {
                    conflicts.push(BindingConflict {
                        context: entry.context.clone(),
                        binding: *binding,
                        actions,
                    });
                }
            }
        }
        conflicts
    }

    /// Returns the actions in a context, other than `action`, that read the given input.
    pub fn conflicts_with(&self, context: &InputContext, action: A, binding: &Binding) -> Vec<A> {
        let mut actions = Vec::new();
        for (bound_context, bound, existing) in self.iter() {
            if bound_context == context
                && bound != action
                && existing.overlaps(binding)
                && !actions.contains(&bound)
            {
                actions.push(bound);
            }
        }
        actions
    }

    /// Creates a serializable snapshot of the bindings.
    pub fn save(&self) -> SavedBindings {
        let contexts = self
            .contexts
            .iter()
            .map(|entry| {
                let mut actions: Vec<(String, Vec<Binding>)> = Vec::new();
                for (action, binding) in &entry.bindings {
                    if is_virtual(binding) {
                        continue;
                    }
                    let name = action.name().to_string();
                    match actions.iter_mut().find(|(existing, _)| *existing == name) {
                        Some((_, bindings)) => bindings.push(*binding),
                        None => actions.push((name, vec![*binding])),
                    }
                }
                SavedContext {
                    context: entry.context.clone(),
                    actions,
                }
            })
            .collect();
        SavedBindings { contexts }
    }

    /// Applies saved bindings. Each saved action replaces that action's non-virtual bindings
    /// in its context; actions absent from the snapshot keep their current bindings.
    ///
    /// Returns the names of saved actions that no longer exist.
    pub fn load(&mut self, saved: &SavedBindings) -> Vec<String> {
        self.capture_defaults();
        let mut unknown = Vec::new();
        for saved_context in &saved.contexts {
            let index = self.context_index_or_insert(saved_context.context.clone());
            let bindings = &mut self.contexts[index].bindings;
            for (name, saved_bindings) in &saved_context.actions {
                let Some(action) = A::from_name(name) else {
                    unknown.push(name.clone());
                    continue;
                };
                bindings.retain(|(bound, binding)| *bound != action || is_virtual(binding));
                bindings.extend(saved_bindings.iter().map(|binding| (action, *binding)));
            }
        }
        unknown
    }

    pub(crate) fn context_bindings(&self) -> impl Iterator<Item = (&InputContext, &[(A, Binding)])> {
        self.contexts
            .iter()
            .map(|entry| (&entry.context, entry.bindings.as_slice()))
    }

    fn context_index_or_insert(&mut self, context: InputContext) -> usize {
        if let Some(index) = self.contexts.iter().position(|entry| entry.context == context) {
            return index;
        }
        self.contexts.push(ContextBindings {
            context,
            bindings: Vec::new(),
        });
        self.contexts.len() - 1
    }

    fn capture_defaults(&mut self) {
        if self.defaults.is_none() {
            self.defaults = Some(self.contexts.clone());
        }
    }
}

fn is_virtual(binding: &Binding) -> bool {
    matches!(
        binding,
        Binding::Button(ButtonBinding::Virtual(_))
            | Binding::Axis(AxisBinding::Virtual(_))
            | Binding::Axis2(Axis2Binding::Virtual(_))
    )
}
