//! Typed asset collections and loading groups.
//!
//! An [`AssetCollection`] is a resource whose fields are handles loaded from fixed paths:
//!
//! ```ignore
//! #[derive(Resource, AssetCollection)]
//! struct GameAssets {
//!     #[asset(path = "fonts/interface.ttf")]
//!     ui_font: Handle<Font>,
//!     #[asset(path = "world/level.glb#Scene0")]
//!     level_scene: Handle<WorldAsset>,
//! }
//! ```
//!
//! An [`AssetGroup`] declares everything one part of the game needs: collections, individual
//! files, other groups it depends on, and initialization steps. A group becomes ready only when
//! every required asset and its dependencies have loaded and every initialization step has
//! finished. Collections are inserted as resources at that moment.
//!
//! ```ignore
//! fn configure(mut assets: ResMut<AssetGroups>) {
//!     assets
//!         .group(AssetGroup::Level)
//!         .depends_on(AssetGroup::Shared)
//!         .collection::<GameAssets>()
//!         .required("world/level.glb")
//!         .optional("audio/ambience.ogg", MissingAssetPolicy::Skip)
//!         .initialize(bind_level_content);
//!
//!     assets.load(AssetGroup::Level);
//! }
//! ```
//!
//! Unloading a group releases its handles; assets shared with another loaded group stay alive
//! through that group's handles. Unloading also cancels pending initialization, so a late step
//! can never populate a group that has already closed.

use std::any::TypeId;
use std::borrow::Cow;
use std::fmt;

use bevy::asset::{LoadState, LoadedUntypedAsset, RecursiveDependencyLoadState, UntypedAssetId};
use bevy::ecs::system::BoxedSystem;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub use bevykit_macros::AssetCollection;

/// A resource of handles loaded from fixed paths. Usually implemented with
/// `#[derive(AssetCollection)]`.
pub trait AssetCollection: Resource + Sized {
    /// Starts loading every handle in the collection.
    fn load(server: &AssetServer) -> Self;

    /// Returns every handle in the collection, for progress and failure tracking.
    fn handles(&self) -> Vec<UntypedHandle>;
}

/// Names a loading group.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Reflect)]
pub struct AssetGroup(Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl AssetGroup {
    /// Content used everywhere, such as fonts and interface sounds.
    pub const Shared: Self = Self::from_static("shared");
    /// Content used by menus.
    pub const Menu: Self = Self::from_static("menu");
    /// Content used by a level.
    pub const Level: Self = Self::from_static("level");

    /// Creates a group name from a static string.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// Creates a group name.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    /// Returns the name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AssetGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "AssetGroup({})", self.0)
    }
}

impl From<&'static str> for AssetGroup {
    fn from(name: &'static str) -> Self {
        Self::from_static(name)
    }
}

/// What to do when an optional asset fails to load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingAssetPolicy {
    /// Leave the asset out; the group can still become ready.
    Skip,
    /// Load this path instead.
    Fallback(Cow<'static, str>),
}

/// An asset that failed to load.
#[derive(Clone, Debug)]
pub struct AssetFailure {
    /// The path that failed.
    pub path: String,
    /// What requested the path: a collection type, or `required`/`optional` declarations.
    pub requested_by: String,
    /// The loader's error.
    pub error: String,
}

impl fmt::Display for AssetFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` (requested by {}): {}",
            self.path, self.requested_by, self.error
        )
    }
}

/// The state of a group.
#[derive(Clone, Debug, Default)]
pub enum GroupStatus {
    /// Not requested.
    #[default]
    Unloaded,
    /// Assets are loading.
    Loading,
    /// Assets are loaded; initialization steps are running.
    Initializing,
    /// Everything has loaded and initialized.
    Ready,
    /// One or more required assets failed.
    Failed(Vec<AssetFailure>),
}

/// Identifies one load of a group. Becomes stale when the group is unloaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadRequest {
    /// The group.
    pub group: AssetGroup,
    generation: u64,
}

/// Sent when a group becomes ready.
#[derive(Message, Clone, Debug)]
pub struct AssetGroupReady(pub AssetGroup);

/// Sent when a group fails.
#[derive(Message, Clone, Debug)]
pub struct AssetGroupFailed {
    /// The group.
    pub group: AssetGroup,
    /// The failures.
    pub failures: Vec<AssetFailure>,
}

trait ErasedCollection: Send + Sync {
    fn collection_type(&self) -> TypeId;
    fn type_name(&self) -> &'static str;
    fn load(&self, server: &AssetServer) -> LoadedCollection;
    fn remove(&self, world: &mut World);
}

struct CollectionEntry<C: AssetCollection>(std::marker::PhantomData<C>);

struct LoadedCollection {
    handles: Vec<UntypedHandle>,
    insert: Option<Box<dyn FnOnce(&mut World) + Send + Sync>>,
}

impl<C: AssetCollection> ErasedCollection for CollectionEntry<C> {
    fn collection_type(&self) -> TypeId {
        TypeId::of::<C>()
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<C>()
    }

    fn load(&self, server: &AssetServer) -> LoadedCollection {
        let collection = C::load(server);
        LoadedCollection {
            handles: collection.handles(),
            insert: Some(Box::new(move |world: &mut World| {
                world.insert_resource(collection);
            })),
        }
    }

    fn remove(&self, world: &mut World) {
        world.remove_resource::<C>();
    }
}

enum Declared {
    Untyped(String),
    Typed(String, fn(&AssetServer, &str) -> UntypedHandle),
}

impl Declared {
    fn path(&self) -> &str {
        match self {
            Declared::Untyped(path) | Declared::Typed(path, _) => path,
        }
    }

    fn load(&self, server: &AssetServer) -> UntypedHandle {
        match self {
            Declared::Untyped(path) => server
                .load_builder()
                .load_untyped(path.clone())
                .untyped(),
            Declared::Typed(path, load) => load(server, path),
        }
    }
}

#[derive(Default)]
struct GroupDefinition {
    dependencies: Vec<AssetGroup>,
    collections: Vec<Box<dyn ErasedCollection>>,
    required: Vec<Declared>,
    optional: Vec<(Declared, MissingAssetPolicy)>,
    initializers: Vec<BoxedSystem<(), bool>>,
}

struct Tracked {
    handle: UntypedHandle,
    path: String,
    requested_by: String,
    policy: Option<MissingAssetPolicy>,
    resolved: bool,
}

#[derive(Default)]
struct GroupState {
    status: GroupStatus,
    generation: u64,
    tracked: Vec<Tracked>,
    pending_inserts: Vec<Box<dyn FnOnce(&mut World) + Send + Sync>>,
    next_initializer: usize,
    initializers_ready: bool,
    requested: bool,
}

/// Declares loading groups and tracks their state.
#[derive(Resource, Default)]
pub struct AssetGroups {
    definitions: HashMap<AssetGroup, GroupDefinition>,
    states: HashMap<AssetGroup, GroupState>,
    unload_requests: Vec<AssetGroup>,
    retry_requests: Vec<AssetGroup>,
}

/// Builder for a group's declaration. Returned by [`AssetGroups::group`].
pub struct GroupBuilder<'a> {
    definition: &'a mut GroupDefinition,
}

impl GroupBuilder<'_> {
    /// Requires another group to be ready first. Loading this group loads the dependency.
    pub fn depends_on(&mut self, group: impl Into<AssetGroup>) -> &mut Self {
        let group = group.into();
        if !self.definition.dependencies.contains(&group) {
            self.definition.dependencies.push(group);
        }
        self
    }

    /// Loads a collection with the group and inserts it as a resource when the group is ready.
    pub fn collection<C: AssetCollection>(&mut self) -> &mut Self {
        if !self
            .definition
            .collections
            .iter()
            .any(|collection| collection.collection_type() == TypeId::of::<C>())
        {
            self.definition
                .collections
                .push(Box::new(CollectionEntry::<C>(std::marker::PhantomData)));
        }
        self
    }

    /// Requires a file of any type. The group fails if it fails.
    pub fn required(&mut self, path: impl Into<String>) -> &mut Self {
        self.definition.required.push(Declared::Untyped(path.into()));
        self
    }

    /// Requires a file loaded as asset type `A`.
    pub fn required_typed<A: Asset>(&mut self, path: impl Into<String>) -> &mut Self {
        self.definition.required.push(Declared::Typed(path.into(), |server, path| {
            server.load::<A>(path.to_string()).untyped()
        }));
        self
    }

    /// Requests a file whose failure is handled by `policy` instead of failing the group.
    pub fn optional(&mut self, path: impl Into<String>, policy: MissingAssetPolicy) -> &mut Self {
        self.definition
            .optional
            .push((Declared::Untyped(path.into()), policy));
        self
    }

    /// Adds an initialization step that runs once, after every asset has loaded.
    pub fn initialize<M>(&mut self, system: impl IntoSystem<(), (), M>) -> &mut Self {
        let system = IntoSystem::into_system(system.map(|()| true));
        self.definition.initializers.push(Box::new(system));
        self
    }

    /// Adds an initialization step that runs every frame until it returns `true`.
    pub fn initialize_until<M>(&mut self, system: impl IntoSystem<(), bool, M>) -> &mut Self {
        self.definition
            .initializers
            .push(Box::new(IntoSystem::into_system(system)));
        self
    }
}

impl AssetGroups {
    /// Returns the declaration builder for a group, creating it if needed.
    pub fn group(&mut self, group: impl Into<AssetGroup>) -> GroupBuilder<'_> {
        GroupBuilder {
            definition: self.definitions.entry(group.into()).or_default(),
        }
    }

    /// Requests that a group and its dependencies load.
    pub fn load(&mut self, group: impl Into<AssetGroup>) -> LoadRequest {
        let group = group.into();
        let dependencies = self
            .definitions
            .get(&group)
            .map(|definition| definition.dependencies.clone())
            .unwrap_or_default();
        for dependency in dependencies {
            self.load(dependency);
        }
        let state = self.states.entry(group.clone()).or_default();
        if matches!(state.status, GroupStatus::Unloaded) && !state.requested {
            state.requested = true;
            state.status = GroupStatus::Loading;
        }
        LoadRequest {
            group,
            generation: state.generation,
        }
    }

    /// Unloads a group: cancels pending initialization, removes its collections, and releases
    /// its handles. Dependencies stay loaded.
    pub fn unload(&mut self, group: impl Into<AssetGroup>) {
        let group = group.into();
        if let Some(state) = self.states.get_mut(&group) {
            state.generation += 1;
            state.requested = false;
        }
        self.unload_requests.push(group);
    }

    /// Reloads the failed assets of a failed group.
    pub fn retry(&mut self, group: impl Into<AssetGroup>) {
        self.retry_requests.push(group.into());
    }

    /// Returns the status of a group.
    pub fn status(&self, group: &AssetGroup) -> GroupStatus {
        self.states
            .get(group)
            .map(|state| state.status.clone())
            .unwrap_or_default()
    }

    /// Returns `true` if the group is ready.
    pub fn is_ready(&self, group: &AssetGroup) -> bool {
        matches!(self.status(group), GroupStatus::Ready)
    }

    /// Returns `true` if the request still refers to the current load of its group.
    pub fn is_current(&self, request: &LoadRequest) -> bool {
        self.states
            .get(&request.group)
            .is_some_and(|state| state.generation == request.generation && state.requested)
    }

    /// Returns the fraction of the group's assets that have loaded, including dependencies.
    pub fn progress(&self, group: &AssetGroup, server: &AssetServer) -> f32 {
        let Some(state) = self.states.get(group) else {
            return 0.0;
        };
        if matches!(state.status, GroupStatus::Ready) {
            return 1.0;
        }
        let total = state.tracked.len();
        if total == 0 {
            return 0.0;
        }
        let loaded = state
            .tracked
            .iter()
            .filter(|tracked| {
                tracked.resolved
                    || server.is_loaded_with_dependencies(tracked.handle.id())
            })
            .count();
        let fraction = loaded as f32 / total as f32;
        if matches!(state.status, GroupStatus::Initializing) {
            fraction.min(0.99)
        } else {
            fraction
        }
    }

    /// Returns the typed handle loaded for a declared path once the group is ready.
    pub fn typed_handle<A: Asset>(&self, group: &AssetGroup, path: &str) -> Option<Handle<A>> {
        self.handle(group, path)?.try_typed::<A>().ok()
    }

    /// Returns the handle loaded for a declared path, if the group has loaded it.
    pub fn handle(&self, group: &AssetGroup, path: &str) -> Option<UntypedHandle> {
        self.states
            .get(group)?
            .tracked
            .iter()
            .find(|tracked| tracked.path == path)
            .map(|tracked| tracked.handle.clone())
    }
}

/// Run condition that is `true` while the group is ready.
pub fn asset_group_ready(group: impl Into<AssetGroup>) -> impl FnMut(Res<AssetGroups>) -> bool {
    let group = group.into();
    move |groups: Res<AssetGroups>| groups.is_ready(&group)
}

fn failure_message(state: &LoadState, recursive: &RecursiveDependencyLoadState) -> Option<String> {
    if let LoadState::Failed(error) = state {
        return Some(error.to_string());
    }
    if let RecursiveDependencyLoadState::Failed(error) = recursive {
        return Some(format!("dependency failed: {error}"));
    }
    None
}

fn tracked_state(server: &AssetServer, id: UntypedAssetId) -> (bool, Option<String>) {
    let state = server.load_state(id);
    let recursive = server.recursive_dependency_load_state(id);
    let loaded = matches!(recursive, RecursiveDependencyLoadState::Loaded);
    (loaded, failure_message(&state, &recursive))
}

/// Advances every group. Runs exclusively in `PreUpdate`.
pub(crate) fn process_asset_groups(world: &mut World) {
    world.resource_scope(|world, mut groups: Mut<AssetGroups>| {
        let groups = &mut *groups;
        let server = world.resource::<AssetServer>().clone();

        for group in std::mem::take(&mut groups.unload_requests) {
            unload_group(world, groups, &group);
        }

        for group in std::mem::take(&mut groups.retry_requests) {
            if let Some(state) = groups.states.get_mut(&group)
                && matches!(state.status, GroupStatus::Failed(_))
            {
                for tracked in &state.tracked {
                    if tracked_state(&server, tracked.handle.id()).1.is_some() {
                        server.reload(tracked.path.clone());
                    }
                }
                state.status = GroupStatus::Loading;
            }
        }

        let mut names: Vec<AssetGroup> = groups.states.keys().cloned().collect();
        names.sort();
        for name in names {
            advance_group(world, groups, &server, &name);
        }
    });
}

fn unload_group(world: &mut World, groups: &mut AssetGroups, group: &AssetGroup) {
    let Some(state) = groups.states.get_mut(group) else {
        return;
    };
    // A group unloaded and requested again in the same frame starts over.
    state.status = if state.requested {
        GroupStatus::Loading
    } else {
        GroupStatus::Unloaded
    };
    state.tracked.clear();
    state.pending_inserts.clear();
    state.next_initializer = 0;
    state.initializers_ready = false;

    let Some(definition) = groups.definitions.get(group) else {
        return;
    };
    for collection in &definition.collections {
        // Keep collections that another live group also declares.
        let shared = groups.definitions.iter().any(|(other, other_definition)| {
            other != group
                && groups
                    .states
                    .get(other)
                    .is_some_and(|state| state.requested)
                && other_definition
                    .collections
                    .iter()
                    .any(|candidate| candidate.collection_type() == collection.collection_type())
        });
        if !shared {
            collection.remove(world);
        }
    }
}

fn advance_group(
    world: &mut World,
    groups: &mut AssetGroups,
    server: &AssetServer,
    name: &AssetGroup,
) {
    let dependencies_ready = groups
        .definitions
        .get(name)
        .map(|definition| {
            definition
                .dependencies
                .iter()
                .all(|dependency| groups.is_ready(dependency))
        })
        .unwrap_or(true);
    let Some(definition) = groups.definitions.get_mut(name) else {
        if let Some(state) = groups.states.get_mut(name)
            && state.requested
            && matches!(state.status, GroupStatus::Loading)
        {
            warn!("Asset group {name:?} was loaded but never declared");
            state.status = GroupStatus::Ready;
        }
        return;
    };
    let Some(state) = groups.states.get_mut(name) else {
        return;
    };
    if !state.requested {
        return;
    }

    if matches!(state.status, GroupStatus::Loading) && state.tracked.is_empty() {
        start_loading(definition, state, server);
    }

    if matches!(state.status, GroupStatus::Loading) {
        let mut failures = Vec::new();
        let mut all_loaded = true;
        let mut fallbacks = Vec::new();
        for tracked in &mut state.tracked {
            if tracked.resolved {
                continue;
            }
            let (loaded, failure) = tracked_state(server, tracked.handle.id());
            match (failure, &tracked.policy) {
                (Some(error), None) => failures.push(AssetFailure {
                    path: tracked.path.clone(),
                    requested_by: tracked.requested_by.clone(),
                    error,
                }),
                (Some(error), Some(MissingAssetPolicy::Skip)) => {
                    warn!("Optional asset `{}` skipped: {error}", tracked.path);
                    tracked.resolved = true;
                }
                (Some(error), Some(MissingAssetPolicy::Fallback(path))) => {
                    warn!(
                        "Optional asset `{}` replaced by `{path}`: {error}",
                        tracked.path
                    );
                    tracked.resolved = true;
                    fallbacks.push(Tracked {
                        handle: Declared::Untyped(path.to_string()).load(server),
                        path: path.to_string(),
                        requested_by: format!("fallback for `{}`", tracked.path),
                        policy: None,
                        resolved: false,
                    });
                    all_loaded = false;
                }
                (None, _) => all_loaded &= loaded,
            }
        }
        state.tracked.extend(fallbacks);

        if !failures.is_empty() {
            for failure in &failures {
                error!("Asset group {name:?} failed: {failure}");
            }
            state.status = GroupStatus::Failed(failures.clone());
            world.write_message(AssetGroupFailed {
                group: name.clone(),
                failures,
            });
            return;
        }
        if all_loaded && dependencies_ready {
            resolve_untyped_handles(world, state);
            for insert in state.pending_inserts.drain(..) {
                insert(world);
            }
            state.status = GroupStatus::Initializing;
        }
    }

    if matches!(state.status, GroupStatus::Initializing) {
        if !state.initializers_ready {
            for initializer in &mut definition.initializers {
                initializer.initialize(world);
            }
            state.initializers_ready = true;
        }
        while let Some(initializer) = definition.initializers.get_mut(state.next_initializer) {
            match initializer.run((), world) {
                Ok(true) => state.next_initializer += 1,
                Ok(false) => return,
                Err(error) => {
                    let failure = AssetFailure {
                        path: String::new(),
                        requested_by: format!("initializer {}", state.next_initializer),
                        error: error.to_string(),
                    };
                    state.status = GroupStatus::Failed(vec![failure.clone()]);
                    world.write_message(AssetGroupFailed {
                        group: name.clone(),
                        failures: vec![failure],
                    });
                    return;
                }
            }
        }
        state.status = GroupStatus::Ready;
        world.write_message(AssetGroupReady(name.clone()));
    }
}

/// Replaces the indirection handles of untyped loads with the handles of the loaded assets.
fn resolve_untyped_handles(world: &World, state: &mut GroupState) {
    let Some(untyped) = world.get_resource::<Assets<LoadedUntypedAsset>>() else {
        return;
    };
    for tracked in &mut state.tracked {
        if let Ok(wrapper) = tracked.handle.clone().try_typed::<LoadedUntypedAsset>()
            && let Some(loaded) = untyped.get(&wrapper)
        {
            tracked.handle = loaded.handle.clone();
        }
    }
}

fn start_loading(definition: &GroupDefinition, state: &mut GroupState, server: &AssetServer) {
    state.next_initializer = 0;
    for collection in &definition.collections {
        let mut loaded = collection.load(server);
        for handle in loaded.handles.drain(..) {
            let path = handle
                .path()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("{:?}", handle.id()));
            state.tracked.push(Tracked {
                handle,
                path,
                requested_by: format!("collection {}", collection.type_name()),
                policy: None,
                resolved: false,
            });
        }
        if let Some(insert) = loaded.insert.take() {
            state.pending_inserts.push(insert);
        }
    }
    for declared in &definition.required {
        state.tracked.push(Tracked {
            handle: declared.load(server),
            path: declared.path().to_string(),
            requested_by: "required".to_string(),
            policy: None,
            resolved: false,
        });
    }
    for (declared, policy) in &definition.optional {
        state.tracked.push(Tracked {
            handle: declared.load(server),
            path: declared.path().to_string(),
            requested_by: "optional".to_string(),
            policy: Some(policy.clone()),
            resolved: false,
        });
    }
}
