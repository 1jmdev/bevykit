//! Data bindings from resources to widgets.
//!
//! A binding reads a value from a resource and applies it to a widget. It is evaluated only when
//! the resource changes, and applied only when the value differs from the last one, so bound
//! screens cost nothing while their data is unchanged.
//!
//! ```ignore
//! ui.progress().bind_resource(|job: &JobStatus| job.progress());
//! ui.button(tr!("job.collect"))
//!     .enabled_when(|job: &JobStatus| job.ready)
//!     .send(CollectJob);
//! ```

use std::sync::Arc;

use bevy::prelude::*;

use crate::builder::WidgetBuilder;
use crate::state::WidgetState;

type Evaluate = Box<dyn FnMut(&World, bool) -> Option<Box<dyn FnOnce(&mut EntityWorldMut) + Send>> + Send + Sync>;

/// One binding on a widget.
pub struct Binding {
    evaluate: Evaluate,
}

impl Binding {
    /// Binds a value computed from resource `R`, applying it with `apply` whenever it changes.
    pub fn resource<R, V>(
        read: impl Fn(&R) -> V + Send + Sync + 'static,
        apply: fn(&mut EntityWorldMut, V),
    ) -> Self
    where
        R: Resource,
        V: PartialEq + Clone + Send + Sync + 'static,
    {
        let mut last: Option<V> = None;
        let read = Arc::new(read);
        Self {
            evaluate: Box::new(move |world: &World, force: bool| {
                if !force && last.is_some() && !world.is_resource_changed::<R>() {
                    return None;
                }
                let value = read(world.get_resource::<R>()?);
                if last.as_ref() == Some(&value) {
                    return None;
                }
                last = Some(value.clone());
                Some(Box::new(move |entity: &mut EntityWorldMut| apply(entity, value)))
            }),
        }
    }
}

/// The bindings of a widget.
#[derive(Component, Default)]
pub struct Bindings(Vec<Binding>);

impl Bindings {
    /// Adds a binding.
    pub fn push(&mut self, binding: Binding) {
        self.0.push(binding);
    }
}

/// Adds a binding to a widget under construction.
pub(crate) fn add_binding(entity: &mut EntityCommands, binding: Binding) {
    entity
        .entry::<Bindings>()
        .or_default()
        .and_modify(move |mut bindings| bindings.push(binding));
}

/// Evaluates every binding and applies changed values. Runs exclusively before layout.
pub(crate) fn evaluate_bindings(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<Bindings>>();
    let entities: Vec<Entity> = query.iter(world).collect();
    let mut updates: Vec<(Entity, Box<dyn FnOnce(&mut EntityWorldMut) + Send>)> = Vec::new();

    for entity in entities {
        // Temporarily take the bindings so they can read the world.
        let Some(mut bindings) = world
            .get_mut::<Bindings>(entity)
            .map(|mut bindings| std::mem::take(&mut bindings.0))
        else {
            continue;
        };
        let force = world
            .get_entity(entity)
            .ok()
            .and_then(|entity| entity.get_ref::<Bindings>())
            .is_some_and(|bindings| bindings.is_added());
        for binding in &mut bindings {
            if let Some(update) = (binding.evaluate)(world, force) {
                updates.push((entity, update));
            }
        }
        if let Some(mut slot) = world.get_mut::<Bindings>(entity) {
            let added = std::mem::take(&mut slot.0);
            slot.bypass_change_detection().0 = bindings;
            slot.bypass_change_detection().0.extend(added);
        }
    }

    for (entity, update) in updates {
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            update(&mut entity);
        }
    }
}

/// Binding methods available on every widget builder.
pub trait BindingExt<'a>: WidgetBuilder<'a> {
    /// Enables the widget only while `condition` holds for resource `R`.
    fn enabled_when<R: Resource>(
        &mut self,
        condition: impl Fn(&R) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            self.entity_commands(),
            Binding::resource(condition, |entity, enabled: bool| {
                if let Some(mut state) = entity.get_mut::<WidgetState>() {
                    state.disabled = !enabled;
                }
            }),
        );
        self
    }

    /// Shows the widget only while `condition` holds for resource `R`. Hidden widgets take
    /// no space in the layout.
    fn visible_when<R: Resource>(
        &mut self,
        condition: impl Fn(&R) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            self.entity_commands(),
            Binding::resource(condition, |entity, visible: bool| {
                if let Some(mut node) = entity.get_mut::<Node>() {
                    node.display = if visible { Display::Flex } else { Display::None };
                }
            }),
        );
        self
    }

    /// Adds a custom binding.
    fn bind(&mut self, binding: Binding) -> &mut Self {
        add_binding(self.entity_commands(), binding);
        self
    }
}

impl<'a, T: WidgetBuilder<'a>> BindingExt<'a> for T {}
