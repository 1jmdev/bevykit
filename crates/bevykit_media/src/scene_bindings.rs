//! Components attached to named nodes of a spawned scene.
//!
//! Place [`SceneBindings`] next to a [`WorldAssetRoot`](bevy::world_serialization::WorldAssetRoot).
//! When the instance is ready, its descendants are searched by [`Name`] once, every binding is
//! inserted, and the result is cached in [`SceneNodes`] on the root, so it is dropped together
//! with the instance.
//!
//! ```ignore
//! commands.spawn((
//!     WorldAssetRoot(assets.load("door.glb#Scene0")),
//!     SceneBindings::new()
//!         .require_node("InteractionPoint", InteractionTarget)
//!         .optional_node("Light", Flicker::default())
//!         .on_ready(|In(root): In<Entity>| info!("door {root} ready")),
//! ));
//! ```

use std::mem;

use bevy::ecs::system::RunSystemOnce;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

type InsertFn = Box<dyn FnOnce(&mut EntityCommands) + Send + Sync>;
type ReadyFn = Box<dyn FnOnce(&mut World, Entity) + Send + Sync>;

struct NodeBinding {
    name: String,
    required: bool,
    insert: InsertFn,
}

/// Components to insert on named descendants once the scene instance is ready.
#[derive(Component, Default)]
pub struct SceneBindings {
    nodes: Vec<NodeBinding>,
    on_ready: Vec<ReadyFn>,
}

impl SceneBindings {
    /// Creates an empty set of bindings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a bundle on the descendant with the given name. If it is missing or ambiguous,
    /// an error is reported and the `on_ready` systems do not run.
    pub fn require_node(self, name: impl Into<String>, bundle: impl Bundle) -> Self {
        self.node(name.into(), true, bundle)
    }

    /// Inserts a bundle on the descendant with the given name if it exists. Ambiguous names
    /// are reported.
    pub fn optional_node(self, name: impl Into<String>, bundle: impl Bundle) -> Self {
        self.node(name.into(), false, bundle)
    }

    /// Runs a system with the root entity as input once every required node is bound.
    pub fn on_ready<M>(
        mut self,
        system: impl IntoSystem<In<Entity>, (), M> + Send + Sync + 'static,
    ) -> Self {
        self.on_ready.push(Box::new(move |world: &mut World, root| {
            if let Err(error) = world.run_system_once_with(system, root) {
                error!("Scene ready system for {root} failed: {error}");
            }
        }));
        self
    }

    fn node(mut self, name: String, required: bool, bundle: impl Bundle) -> Self {
        self.nodes.push(NodeBinding {
            name,
            required,
            insert: Box::new(move |entity: &mut EntityCommands| {
                entity.insert(bundle);
            }),
        });
        self
    }
}

/// The uniquely named descendants of a scene instance, cached when its bindings resolve.
#[derive(Component, Default, Debug, Clone)]
pub struct SceneNodes(pub HashMap<String, Entity>);

impl SceneNodes {
    /// Returns the descendant with the given name.
    pub fn get(&self, name: &str) -> Option<Entity> {
        self.0.get(name).copied()
    }
}

/// Why a scene binding failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneBindingErrorKind {
    /// No descendant has the name.
    Missing,
    /// Several descendants share the name.
    Ambiguous,
}

/// Sent when a node named by [`SceneBindings`] cannot be bound.
#[derive(Message, Clone, Debug)]
pub struct SceneBindingError {
    /// The scene root.
    pub root: Entity,
    /// The node name.
    pub name: String,
    /// Why the binding failed.
    pub kind: SceneBindingErrorKind,
}

pub(crate) fn resolve_scene_bindings(
    ready: On<WorldInstanceReady>,
    mut roots: Query<&mut SceneBindings>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut errors: MessageWriter<SceneBindingError>,
    mut commands: Commands,
) {
    let root = ready.entity;
    let Ok(mut bindings) = roots.get_mut(root) else {
        return;
    };
    let SceneBindings { nodes, on_ready } = mem::take(&mut *bindings);
    let mut matches = HashMap::<String, Vec<Entity>>::new();
    for entity in children.iter_descendants(root) {
        if let Ok(name) = names.get(entity) {
            matches.entry(name.as_str().to_owned()).or_default().push(entity);
        }
    }

    let mut failed = false;
    for node in nodes {
        let kind = match matches.get(&node.name).map(Vec::as_slice) {
            Some(&[entity]) => {
                (node.insert)(&mut commands.entity(entity));
                continue;
            }
            Some(_) => SceneBindingErrorKind::Ambiguous,
            None if node.required => SceneBindingErrorKind::Missing,
            None => continue,
        };
        error!("Scene node {:?} under {root} is {kind:?}", node.name);
        errors.write(SceneBindingError {
            root,
            name: node.name,
            kind,
        });
        failed |= node.required;
    }

    commands.entity(root).insert(SceneNodes(
        matches
            .into_iter()
            .filter_map(|(name, entities)| match entities[..] {
                [entity] => Some((name, entity)),
                _ => None,
            })
            .collect(),
    ));
    if !failed {
        for ready in on_ready {
            commands.queue(move |world: &mut World| ready(world, root));
        }
    }
}
