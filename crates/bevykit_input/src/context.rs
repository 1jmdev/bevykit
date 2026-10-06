//! Input contexts: which groups of bindings are active at any moment.
//!
//! Bindings belong to a context such as gameplay, a menu, or a dialog. Base contexts are
//! active unless something blocks them. Pushed contexts form a stack; each entry can block
//! specific contexts beneath it or every context beneath it.
//!
//! ```ignore
//! fn open_pause_menu(mut contexts: ResMut<InputContexts>) {
//!     contexts.push(InputContext::PauseMenu).block(InputContext::Gameplay);
//! }
//!
//! fn close_pause_menu(mut contexts: ResMut<InputContexts>) {
//!     contexts.remove(&InputContext::PauseMenu);
//! }
//! ```

use std::borrow::Cow;
use std::fmt;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Names a group of bindings.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Reflect, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputContext(Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl InputContext {
    /// Gameplay controls. Active by default.
    pub const Gameplay: Self = Self::from_static("gameplay");
    /// Generic menu navigation.
    pub const Menu: Self = Self::from_static("menu");
    /// The pause menu.
    pub const PauseMenu: Self = Self::from_static("pause_menu");
    /// Modal dialogs.
    pub const Dialog: Self = Self::from_static("dialog");
    /// Text entry, which usually blocks every other context.
    pub const TextEntry: Self = Self::from_static("text_entry");

    /// Creates a context from a static name.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// Creates a context from any name.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    /// Returns the name of the context.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InputContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "InputContext({})", self.0)
    }
}

impl From<&'static str> for InputContext {
    fn from(name: &'static str) -> Self {
        Self::from_static(name)
    }
}

/// What a pushed context blocks beneath it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ContextBlocking {
    /// Contexts beneath remain active.
    #[default]
    Nothing,
    /// The listed contexts are inactive while this entry is on the stack.
    Contexts(Vec<InputContext>),
    /// Every context beneath is inactive.
    Everything,
}

/// An entry on the context stack.
#[derive(Clone, Debug, Reflect)]
pub struct ContextEntry {
    context: InputContext,
    blocking: ContextBlocking,
}

impl ContextEntry {
    /// Blocks a context beneath this entry.
    pub fn block(&mut self, context: impl Into<InputContext>) -> &mut Self {
        let context = context.into();
        match &mut self.blocking {
            ContextBlocking::Everything => {}
            ContextBlocking::Contexts(contexts) => contexts.push(context),
            ContextBlocking::Nothing => self.blocking = ContextBlocking::Contexts(vec![context]),
        }
        self
    }

    /// Blocks every context beneath this entry.
    pub fn block_all(&mut self) -> &mut Self {
        self.blocking = ContextBlocking::Everything;
        self
    }

    /// Returns the context of this entry.
    pub fn context(&self) -> &InputContext {
        &self.context
    }

    /// Returns what this entry blocks.
    pub fn blocking(&self) -> &ContextBlocking {
        &self.blocking
    }
}

/// The active input contexts.
#[derive(Resource, Clone, Debug, Reflect)]
#[reflect(Resource)]
pub struct InputContexts {
    base: Vec<InputContext>,
    stack: Vec<ContextEntry>,
}

impl Default for InputContexts {
    fn default() -> Self {
        Self {
            base: vec![InputContext::Gameplay],
            stack: Vec::new(),
        }
    }
}

impl InputContexts {
    /// Pushes a context on top of the stack and returns it for configuration.
    ///
    /// Pushing a context that is already on the stack moves it to the top.
    pub fn push(&mut self, context: impl Into<InputContext>) -> &mut ContextEntry {
        let context = context.into();
        self.stack.retain(|entry| entry.context != context);
        self.stack.push(ContextEntry {
            context,
            blocking: ContextBlocking::Nothing,
        });
        self.stack.last_mut().expect("an entry was just pushed")
    }

    /// Removes the topmost context and returns it.
    pub fn pop(&mut self) -> Option<InputContext> {
        self.stack.pop().map(|entry| entry.context)
    }

    /// Removes a context from the stack. Returns `true` if it was present.
    pub fn remove(&mut self, context: &InputContext) -> bool {
        let before = self.stack.len();
        self.stack.retain(|entry| &entry.context != context);
        before != self.stack.len()
    }

    /// Adds a base context, active unless blocked.
    pub fn add_base(&mut self, context: impl Into<InputContext>) {
        let context = context.into();
        if !self.base.contains(&context) {
            self.base.push(context);
        }
    }

    /// Removes a base context.
    pub fn remove_base(&mut self, context: &InputContext) {
        self.base.retain(|base| base != context);
    }

    /// Returns the topmost pushed context.
    pub fn top(&self) -> Option<&InputContext> {
        self.stack.last().map(|entry| &entry.context)
    }

    /// Returns `true` if the context is on the stack or is a base context.
    pub fn contains(&self, context: &InputContext) -> bool {
        self.base.contains(context) || self.stack.iter().any(|entry| &entry.context == context)
    }

    /// Returns `true` if the context is present and not blocked.
    pub fn is_active(&self, context: &InputContext) -> bool {
        let mut blocked_everything = false;
        let mut blocked: Vec<&InputContext> = Vec::new();
        for entry in self.stack.iter().rev() {
            if &entry.context == context {
                return !blocked_everything && !blocked.contains(&context);
            }
            if blocked_everything {
                continue;
            }
            match &entry.blocking {
                ContextBlocking::Nothing => {}
                ContextBlocking::Contexts(contexts) => blocked.extend(contexts.iter()),
                ContextBlocking::Everything => blocked_everything = true,
            }
        }
        self.base.contains(context) && !blocked_everything && !blocked.contains(&context)
    }

    /// Returns every active context, topmost first.
    pub fn active(&self) -> Vec<InputContext> {
        self.stack
            .iter()
            .rev()
            .map(|entry| &entry.context)
            .chain(self.base.iter())
            .filter(|context| self.is_active(context))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushed_context_blocks_listed_contexts_only() {
        let mut contexts = InputContexts::default();
        contexts.add_base("camera");
        contexts.push(InputContext::PauseMenu).block(InputContext::Gameplay);

        assert!(contexts.is_active(&InputContext::PauseMenu));
        assert!(!contexts.is_active(&InputContext::Gameplay));
        assert!(contexts.is_active(&InputContext::from_static("camera")));

        contexts.remove(&InputContext::PauseMenu);
        assert!(contexts.is_active(&InputContext::Gameplay));
    }

    #[test]
    fn block_all_hides_everything_beneath() {
        let mut contexts = InputContexts::default();
        contexts.push(InputContext::Menu);
        contexts.push(InputContext::Dialog).block_all();

        assert_eq!(contexts.active(), vec![InputContext::Dialog]);
    }
}
