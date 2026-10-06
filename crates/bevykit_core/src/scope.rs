//! Lifetime scopes that own entities, tasks, and nested scopes.
//!
//! A scope is an ordinary entity carrying the [`Scope`] component. Anything spawned with
//! [`OwnedBy`] pointing at that entity is despawned when the scope closes, and nested scopes
//! close together with their parent. Because ownership is expressed as a Bevy relationship,
//! scopes compose with every other ECS feature: queries, observers, and hierarchy tools.
//!
//! ```ignore
//! fn start_level(mut scopes: Scopes, mut commands: Commands) {
//!     let scope = scopes.create("level-session");
//!     commands.spawn((LevelObject, OwnedBy(scope)));
//! }
//!
//! fn end_level(mut scopes: Scopes, levels: Query<Entity, With<LevelScope>>) {
//!     for scope in &levels {
//!         scopes.close(scope);
//!     }
//! }
//! ```

use std::borrow::Cow;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Marks an entity as a lifetime scope.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, Debug)]
#[require(ScopeOwned)]
pub struct Scope {
    name: Cow<'static, str>,
}

impl Scope {
    /// Creates a scope descriptor with a diagnostic name.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self { name: name.into() }
    }

    /// Returns the diagnostic name of the scope.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Declares that an entity belongs to a scope and is despawned when the scope closes.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Reflect)]
#[reflect(Component, Debug, PartialEq)]
#[relationship(relationship_target = ScopeOwned)]
pub struct OwnedBy(pub Entity);

impl OwnedBy {
    /// Returns the owning scope.
    pub fn scope(&self) -> Entity {
        self.0
    }
}

/// The set of entities owned by a scope. Maintained automatically from [`OwnedBy`].
#[derive(Component, Default, Debug, Reflect)]
#[reflect(Component, Debug)]
#[relationship_target(relationship = OwnedBy, linked_spawn)]
pub struct ScopeOwned(Vec<Entity>);

impl ScopeOwned {
    /// Returns the entities currently owned by the scope.
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// Triggered on a scope entity immediately before it is closed.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ScopeClosing {
    /// The scope being closed.
    pub entity: Entity,
}

/// System parameter for creating and closing scopes.
#[derive(SystemParam)]
pub struct Scopes<'w, 's> {
    commands: Commands<'w, 's>,
    scopes: Query<'w, 's, (Entity, &'static Scope)>,
}

impl Scopes<'_, '_> {
    /// Creates a new top-level scope and returns its entity.
    pub fn create(&mut self, name: impl Into<Cow<'static, str>>) -> Entity {
        self.commands.spawn(Scope::new(name)).id()
    }

    /// Creates a scope nested inside `parent`. It closes automatically with its parent.
    pub fn create_child(&mut self, parent: Entity, name: impl Into<Cow<'static, str>>) -> Entity {
        self.commands.spawn((Scope::new(name), OwnedBy(parent))).id()
    }

    /// Closes a scope, despawning every entity it owns and cancelling its scoped tasks.
    ///
    /// Closing a scope that no longer exists is a no-op.
    pub fn close(&mut self, scope: Entity) {
        self.commands.queue(CloseScope(scope));
    }

    /// Returns `true` if the entity is a live scope.
    pub fn is_open(&self, scope: Entity) -> bool {
        self.scopes.contains(scope)
    }

    /// Finds the first open scope with the given name.
    pub fn find(&self, name: &str) -> Option<Entity> {
        self.scopes
            .iter()
            .find(|(_, scope)| scope.name() == name)
            .map(|(entity, _)| entity)
    }
}

/// Command that closes a scope.
#[derive(Clone, Copy, Debug)]
pub struct CloseScope(pub Entity);

impl Command for CloseScope {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Ok(entity) = world.get_entity(self.0) else {
            return;
        };
        if !entity.contains::<Scope>() {
            warn!("CloseScope called on {:?}, which is not a scope", self.0);
            return;
        }
        world.trigger(ScopeClosing { entity: self.0 });
        if let Ok(entity) = world.get_entity_mut(self.0) {
            entity.despawn();
        }
    }
}

/// Extension methods for closing scopes from [`Commands`].
pub trait ScopeCommandsExt {
    /// Closes the given scope.
    fn close_scope(&mut self, scope: Entity);
}

impl ScopeCommandsExt for Commands<'_, '_> {
    fn close_scope(&mut self, scope: Entity) {
        self.queue(CloseScope(scope));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_scope_despawns_owned_entities_recursively() {
        let mut world = World::new();
        let outer = world.spawn(Scope::new("session")).id();
        let inner = world.spawn((Scope::new("level"), OwnedBy(outer))).id();
        let object = world.spawn(OwnedBy(inner)).id();
        let survivor = world.spawn_empty().id();

        CloseScope(outer).apply(&mut world);

        assert!(world.get_entity(outer).is_err());
        assert!(world.get_entity(inner).is_err());
        assert!(world.get_entity(object).is_err());
        assert!(world.get_entity(survivor).is_ok());
    }
}
