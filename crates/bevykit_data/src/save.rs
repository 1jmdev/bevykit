//! Versioned save data with slots, migrations, backups, and recovery.
//!
//! The game owns its save structures; bevykit stores them. A save type derives [`SaveData`]
//! and registers a *snapshot* system that captures it and a *restore* system that applies it.
//! An optional *link* system runs after restored entities exist, for reconnecting references
//! through [`SaveId`]s.
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, SaveData)]
//! #[save(version = 3)]
//! struct GameSave {
//!     player: PlayerSave,
//!     completed_levels: Vec<String>,
//! }
//!
//! app.add_plugins(KitSavePlugin::default())
//!     .register_save::<GameSave>()
//!     .snapshot(capture_game)
//!     .restore(restore_game)
//!     .migrate(2, migrate_v2_to_v3);
//!
//! fn checkpoint(mut saves: ResMut<Saves>) {
//!     saves.request(SaveSlot::Auto);
//! }
//! ```
//!
//! Snapshots are captured synchronously, so they are consistent, and written asynchronously.
//! Writes to a slot are ordered: an older request never overwrites newer progress. A file that
//! fails to parse or migrate falls back to its backup; if both fail, the current session is
//! left untouched and the failure is reported.

use std::borrow::Cow;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::system::BoxedSystem;
use bevy::ecs::world::DeferredWorld;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{IoTaskPool, Task};
use bevy::window::AppLifecycle;
use bevykit_core::deadline::{Deadlines, WallTime};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::storage::{KitStorage, PendingWrite, StorageError, ensure_storage};

pub use bevykit_macros::SaveData;

/// A game-defined save structure. Usually implemented with `#[derive(SaveData)]`.
pub trait SaveData: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The current version. Older versions are upgraded by registered migrations.
    const VERSION: u32;

    /// The name of this entry within a save file.
    const NAME: &'static str;
}

/// A migration from one version's JSON representation to the next.
pub type Migration = fn(serde_json::Value) -> Result<serde_json::Value, String>;

/// A save slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub enum SaveSlot {
    /// The automatic checkpoint slot.
    Auto,
    /// The quick-save slot.
    Quick,
    /// A named slot.
    Named(Cow<'static, str>),
}

impl SaveSlot {
    /// Creates a named slot.
    pub fn named(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Named(name.into())
    }

    /// Returns the storage record for this slot.
    pub fn storage_key(&self) -> String {
        match self {
            SaveSlot::Auto => "save-auto.json".to_string(),
            SaveSlot::Quick => "save-quick.json".to_string(),
            SaveSlot::Named(name) => {
                let sanitized: String = name
                    .chars()
                    .map(|character| {
                        if character.is_ascii_alphanumeric() || character == '-' {
                            character
                        } else {
                            '_'
                        }
                    })
                    .collect();
                format!("save-slot-{sanitized}.json")
            }
        }
    }

    fn from_storage_key(key: &str) -> Option<Self> {
        match key {
            "save-auto.json" => Some(SaveSlot::Auto),
            "save-quick.json" => Some(SaveSlot::Quick),
            _ => key
                .strip_prefix("save-slot-")?
                .strip_suffix(".json")
                .map(|name| SaveSlot::Named(Cow::Owned(name.to_string()))),
        }
    }
}

/// A failure while saving or loading.
#[derive(Error, Debug, Clone)]
pub enum SaveError {
    /// The storage backend failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The slot holds no save.
    #[error("slot {0:?} is empty")]
    Empty(SaveSlot),
    /// The file could not be parsed.
    #[error("save file is malformed: {0}")]
    Malformed(String),
    /// The file was written by a newer version of the game.
    #[error("entry `{entry}` has version {found}, newer than supported version {supported}")]
    Unsupported {
        /// The entry.
        entry: String,
        /// The version in the file.
        found: u32,
        /// The newest version this build understands.
        supported: u32,
    },
    /// A migration step is missing or failed.
    #[error("entry `{entry}` could not be migrated from version {from}: {message}")]
    Migration {
        /// The entry.
        entry: String,
        /// The version being migrated.
        from: u32,
        /// What went wrong.
        message: String,
    },
    /// A snapshot or restore system failed.
    #[error("save system for `{entry}` failed: {message}")]
    System {
        /// The entry.
        entry: String,
        /// What went wrong.
        message: String,
    },
}

/// Identifies an entity across save and load. Persisted relationships refer to these instead
/// of runtime [`Entity`] values.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[component(on_insert = register_save_id, on_discard = unregister_save_id)]
pub struct SaveId(pub Cow<'static, str>);

impl SaveId {
    /// Creates an identifier.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self(id.into())
    }
}

/// Maps [`SaveId`]s to the entities carrying them. Maintained automatically.
#[derive(Resource, Default, Debug)]
pub struct SaveIds {
    entities: HashMap<SaveId, Entity>,
}

impl SaveIds {
    /// Returns the entity with the given identifier.
    pub fn get(&self, id: &SaveId) -> Option<Entity> {
        self.entities.get(id).copied()
    }

    /// Returns the entity with the given identifier string.
    pub fn find(&self, id: &str) -> Option<Entity> {
        self.entities.get(&SaveId::new(id.to_string())).copied()
    }
}

fn register_save_id(mut world: DeferredWorld, context: HookContext) {
    let Some(id) = world.get::<SaveId>(context.entity).cloned() else {
        return;
    };
    let Some(mut ids) = world.get_resource_mut::<SaveIds>() else {
        return;
    };
    if let Some(previous) = ids.entities.insert(id.clone(), context.entity)
        && previous != context.entity
    {
        warn!("SaveId {:?} is used by both {previous:?} and {:?}", id.0, context.entity);
    }
}

fn unregister_save_id(mut world: DeferredWorld, context: HookContext) {
    let Some(id) = world.get::<SaveId>(context.entity).cloned() else {
        return;
    };
    if let Some(mut ids) = world.get_resource_mut::<SaveIds>()
        && ids.entities.get(&id) == Some(&context.entity)
    {
        ids.entities.remove(&id);
    }
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    format: u32,
    saved_at: WallTime,
    entries: HashMap<String, SaveEntry>,
}

#[derive(Serialize, Deserialize)]
struct SaveEntry {
    version: u32,
    data: serde_json::Value,
}

const FILE_FORMAT: u32 = 1;

/// Type-erased handling of one registered save type.
trait SaveHandler: std::any::Any + Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> u32;
    fn capture(&mut self, world: &mut World) -> Result<serde_json::Value, SaveError>;
    fn decode(&self, entry: SaveEntry) -> Result<Box<dyn std::any::Any + Send>, SaveError>;
    fn restore(
        &mut self,
        world: &mut World,
        value: Box<dyn std::any::Any + Send>,
    ) -> Result<(), SaveError>;
    fn link(&mut self, world: &mut World) -> Result<(), SaveError>;
}

struct TypedHandler<T: SaveData> {
    snapshot: Option<BoxedSystem<(), T>>,
    restore: Option<BoxedSystem<In<T>, ()>>,
    link: Option<BoxedSystem<(), ()>>,
    migrations: HashMap<u32, Migration>,
    initialized: bool,
}

impl<T: SaveData> TypedHandler<T> {
    fn initialize(&mut self, world: &mut World) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        if let Some(system) = &mut self.snapshot {
            system.initialize(world);
        }
        if let Some(system) = &mut self.restore {
            system.initialize(world);
        }
        if let Some(system) = &mut self.link {
            system.initialize(world);
        }
    }

    fn system_error(message: impl ToString) -> SaveError {
        SaveError::System {
            entry: T::NAME.to_string(),
            message: message.to_string(),
        }
    }
}

impl<T: SaveData> SaveHandler for TypedHandler<T> {
    fn name(&self) -> &'static str {
        T::NAME
    }

    fn version(&self) -> u32 {
        T::VERSION
    }

    fn capture(&mut self, world: &mut World) -> Result<serde_json::Value, SaveError> {
        self.initialize(world);
        let system = self
            .snapshot
            .as_mut()
            .ok_or_else(|| Self::system_error("no snapshot system registered"))?;
        let value = system.run((), world).map_err(Self::system_error)?;
        serde_json::to_value(value).map_err(Self::system_error)
    }

    fn decode(&self, entry: SaveEntry) -> Result<Box<dyn std::any::Any + Send>, SaveError> {
        if entry.version > T::VERSION {
            return Err(SaveError::Unsupported {
                entry: T::NAME.to_string(),
                found: entry.version,
                supported: T::VERSION,
            });
        }
        let mut data = entry.data;
        for from in entry.version..T::VERSION {
            let migration = self.migrations.get(&from).ok_or_else(|| SaveError::Migration {
                entry: T::NAME.to_string(),
                from,
                message: "no migration registered".to_string(),
            })?;
            data = migration(data).map_err(|message| SaveError::Migration {
                entry: T::NAME.to_string(),
                from,
                message,
            })?;
        }
        let value: T = serde_json::from_value(data)
            .map_err(|error| SaveError::Malformed(format!("entry `{}`: {error}", T::NAME)))?;
        Ok(Box::new(value))
    }

    fn restore(
        &mut self,
        world: &mut World,
        value: Box<dyn std::any::Any + Send>,
    ) -> Result<(), SaveError> {
        self.initialize(world);
        let value = *value
            .downcast::<T>()
            .map_err(|_| Self::system_error("decoded value has the wrong type"))?;
        let system = self
            .restore
            .as_mut()
            .ok_or_else(|| Self::system_error("no restore system registered"))?;
        system.run(value, world).map_err(Self::system_error)
    }

    fn link(&mut self, world: &mut World) -> Result<(), SaveError> {
        self.initialize(world);
        match &mut self.link {
            Some(system) => system.run((), world).map_err(Self::system_error),
            None => Ok(()),
        }
    }
}

/// Builder returned by [`SaveAppExt::register_save`].
pub struct SaveRegistration<'a, T: SaveData> {
    app: &'a mut App,
    marker: std::marker::PhantomData<T>,
}

impl<T: SaveData> SaveRegistration<'_, T> {
    fn with_handler(&mut self, change: impl FnOnce(&mut TypedHandler<T>)) -> &mut Self {
        let mut saves = self.app.world_mut().resource_mut::<Saves>();
        let handler = saves
            .handlers
            .iter_mut()
            .find(|handler| handler.name() == T::NAME)
            .expect("save type is registered");
        // The handler for `T` is always a `TypedHandler<T>`: it is created by `register_save`.
        let handler = (handler.as_mut() as &mut dyn std::any::Any)
            .downcast_mut::<TypedHandler<T>>();
        if let Some(handler) = handler {
            change(handler);
        }
        self
    }

    /// Sets the system that captures the save data.
    pub fn snapshot<M>(&mut self, system: impl IntoSystem<(), T, M>) -> &mut Self {
        let system: BoxedSystem<(), T> = Box::new(IntoSystem::into_system(system));
        self.with_handler(|handler| handler.snapshot = Some(system))
    }

    /// Sets the system that applies loaded save data.
    pub fn restore<M>(&mut self, system: impl IntoSystem<In<T>, (), M>) -> &mut Self {
        let system: BoxedSystem<In<T>, ()> = Box::new(IntoSystem::into_system(system));
        self.with_handler(|handler| handler.restore = Some(system))
    }

    /// Sets a system that runs after every restore system has run and its commands have been
    /// applied, for resolving [`SaveId`] references between restored entities.
    pub fn link<M>(&mut self, system: impl IntoSystem<(), (), M>) -> &mut Self {
        let system: BoxedSystem<(), ()> = Box::new(IntoSystem::into_system(system));
        self.with_handler(|handler| handler.link = Some(system))
    }

    /// Registers the migration from version `from` to `from + 1`.
    pub fn migrate(&mut self, from: u32, migration: Migration) -> &mut Self {
        self.with_handler(|handler| {
            handler.migrations.insert(from, migration);
        })
    }
}

/// Registers save types on an [`App`].
pub trait SaveAppExt {
    /// Registers a save type. Requires [`KitSavePlugin`].
    fn register_save<T: SaveData>(&mut self) -> SaveRegistration<'_, T>;
}

impl SaveAppExt for App {
    fn register_save<T: SaveData>(&mut self) -> SaveRegistration<'_, T> {
        let mut saves = self
            .world_mut()
            .get_resource_mut::<Saves>()
            .expect("add KitSavePlugin before registering save types");
        if !saves.handlers.iter().any(|handler| handler.name() == T::NAME) {
            saves.handlers.push(Box::new(TypedHandler::<T> {
                snapshot: None,
                restore: None,
                link: None,
                migrations: HashMap::default(),
                initialized: false,
            }));
        }
        SaveRegistration {
            app: self,
            marker: std::marker::PhantomData,
        }
    }
}

/// What the save system is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SaveActivity {
    /// Nothing in progress.
    #[default]
    Idle,
    /// One or more writes are in progress.
    Saving,
    /// A load is in progress.
    Loading,
}

/// Sent when a save request finishes.
#[derive(Message, Clone, Debug)]
pub struct SaveCompleted {
    /// The slot.
    pub slot: SaveSlot,
    /// The outcome.
    pub result: Result<(), SaveError>,
}

/// Sent when a load request finishes.
#[derive(Message, Clone, Debug)]
pub struct LoadCompleted {
    /// The slot.
    pub slot: SaveSlot,
    /// The outcome. On failure the current session is unchanged.
    pub result: Result<(), SaveError>,
    /// `true` if the primary file was unusable and the backup was loaded instead.
    pub recovered: bool,
}

/// Summary of a stored save, from [`Saves::list`].
#[derive(Clone, Debug)]
pub struct SaveSummary {
    /// The slot.
    pub slot: SaveSlot,
    /// When the save was written.
    pub saved_at: WallTime,
}

/// When saves happen automatically.
#[derive(Clone, Debug)]
pub struct AutosavePolicy {
    /// Save to [`SaveSlot::Auto`] at this interval of gameplay, if set.
    pub interval: Option<Duration>,
    /// Save synchronously when the application is about to be suspended.
    pub on_suspend: bool,
}

impl Default for AutosavePolicy {
    fn default() -> Self {
        Self {
            interval: None,
            on_suspend: true,
        }
    }
}

type ReadResult = Result<Option<Vec<u8>>, StorageError>;

/// The save service.
#[derive(Resource)]
pub struct Saves {
    handlers: Vec<Box<dyn SaveHandler>>,
    save_requests: Vec<SaveSlot>,
    load_request: Option<SaveSlot>,
    loading: Option<(SaveSlot, Task<(ReadResult, ReadResult)>)>,
    writes: Vec<(SaveSlot, PendingWrite)>,
    last_error: Option<SaveError>,
    /// Automatic save behavior.
    pub autosave: AutosavePolicy,
    autosave_elapsed: Duration,
}

impl Default for Saves {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
            save_requests: Vec::new(),
            load_request: None,
            loading: None,
            writes: Vec::new(),
            last_error: None,
            autosave: AutosavePolicy::default(),
            autosave_elapsed: Duration::ZERO,
        }
    }
}

impl Saves {
    /// Requests a save to `slot` at the end of the frame.
    pub fn request(&mut self, slot: SaveSlot) {
        if !self.save_requests.contains(&slot) {
            self.save_requests.push(slot);
        }
    }

    /// Requests a load from `slot`. Replaces any load request not yet started.
    pub fn load(&mut self, slot: SaveSlot) {
        self.load_request = Some(slot);
    }

    /// Returns what the save system is doing.
    pub fn activity(&self) -> SaveActivity {
        if self.loading.is_some() || self.load_request.is_some() {
            SaveActivity::Loading
        } else if !self.writes.is_empty() || !self.save_requests.is_empty() {
            SaveActivity::Saving
        } else {
            SaveActivity::Idle
        }
    }

    /// Returns the most recent failure, cleared by the next success.
    pub fn last_error(&self) -> Option<&SaveError> {
        self.last_error.as_ref()
    }

    /// Lists stored saves, newest first.
    pub fn list(storage: &KitStorage) -> Result<Vec<SaveSummary>, SaveError> {
        let mut summaries = Vec::new();
        for key in storage.backend().list("save-")? {
            let Some(slot) = SaveSlot::from_storage_key(&key) else {
                continue;
            };
            let Some(bytes) = storage.read(&key)? else {
                continue;
            };
            #[derive(Deserialize)]
            struct Header {
                saved_at: WallTime,
            }
            if let Ok(header) = serde_json::from_slice::<Header>(&bytes) {
                summaries.push(SaveSummary {
                    slot,
                    saved_at: header.saved_at,
                });
            }
        }
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.saved_at));
        Ok(summaries)
    }

    /// Deletes a stored save and its backup.
    pub fn delete(storage: &KitStorage, slot: &SaveSlot) -> Result<(), SaveError> {
        Ok(storage.backend().delete(&slot.storage_key())?)
    }
}

fn capture_file(world: &mut World, handlers: &mut [Box<dyn SaveHandler>]) -> Result<Vec<u8>, SaveError> {
    let mut entries = HashMap::default();
    for handler in handlers.iter_mut() {
        let data = handler.capture(world)?;
        entries.insert(
            handler.name().to_string(),
            SaveEntry {
                version: handler.version(),
                data,
            },
        );
    }
    let saved_at = world
        .get_resource::<Deadlines>()
        .map(Deadlines::now)
        .unwrap_or_default();
    let file = SaveFile {
        format: FILE_FORMAT,
        saved_at,
        entries,
    };
    serde_json::to_vec(&file).map_err(|error| SaveError::Malformed(error.to_string()))
}

fn decode_file(
    handlers: &[Box<dyn SaveHandler>],
    bytes: &[u8],
) -> Result<Vec<(usize, Box<dyn std::any::Any + Send>)>, SaveError> {
    let mut file: SaveFile =
        serde_json::from_slice(bytes).map_err(|error| SaveError::Malformed(error.to_string()))?;
    if file.format > FILE_FORMAT {
        return Err(SaveError::Malformed(format!(
            "file format {} is newer than supported format {FILE_FORMAT}",
            file.format
        )));
    }
    let mut decoded = Vec::new();
    for (index, handler) in handlers.iter().enumerate() {
        if let Some(entry) = file.entries.remove(handler.name()) {
            decoded.push((index, handler.decode(entry)?));
        }
    }
    Ok(decoded)
}

/// Processes save and load requests. Runs exclusively at the end of the frame.
fn process_saves(world: &mut World) {
    let suspending = world
        .get_resource_mut::<Messages<AppLifecycle>>()
        .is_some_and(|messages| {
            messages
                .iter_current_update_messages()
                .any(|event| matches!(event, AppLifecycle::WillSuspend))
        });
    let delta = world.resource::<Time<Virtual>>().delta();

    world.resource_scope(|world, mut saves: Mut<Saves>| {
        let saves = &mut *saves;
        let storage = world.resource::<KitStorage>().clone();

        if let Some(interval) = saves.autosave.interval {
            saves.autosave_elapsed += delta;
            if saves.autosave_elapsed >= interval {
                saves.autosave_elapsed = Duration::ZERO;
                saves.request(SaveSlot::Auto);
            }
        }
        if suspending && saves.autosave.on_suspend {
            saves.request(SaveSlot::Auto);
        }

        for slot in std::mem::take(&mut saves.save_requests) {
            let key = slot.storage_key();
            match capture_file(world, &mut saves.handlers) {
                Ok(bytes) if suspending => {
                    // Suspension may leave no time for an asynchronous write to finish.
                    let result = storage.write_blocking(&key, &bytes).map_err(SaveError::from);
                    report_save(world, &mut saves.last_error, slot, result);
                }
                Ok(bytes) => saves.writes.push((slot, storage.write(&key, bytes))),
                Err(error) => report_save(world, &mut saves.last_error, slot, Err(error)),
            }
        }

        let mut finished = Vec::new();
        for (slot, mut write) in std::mem::take(&mut saves.writes) {
            let done = match suspending {
                true => Some(write.wait()),
                false => match write.poll() {
                    Some(done) => Some(done),
                    None => {
                        saves.writes.push((slot.clone(), write));
                        None
                    }
                },
            };
            if let Some(done) = done {
                finished.push((slot, done.result.map_err(SaveError::from)));
            }
        }
        for (slot, result) in finished {
            report_save(world, &mut saves.last_error, slot, result);
        }

        if saves.loading.is_none()
            && let Some(slot) = saves.load_request.take()
        {
            let key = slot.storage_key();
            let reader = storage.clone();
            let task = IoTaskPool::get()
                .spawn(async move { (reader.read(&key), reader.backend().read_backup(&key)) });
            saves.loading = Some((slot, task));
        }

        let ready = match &mut saves.loading {
            Some((_, task)) => check_ready(task),
            None => None,
        };
        if let Some((primary, backup)) = ready {
            let (slot, _) = saves.loading.take().expect("a load is in progress");
            let outcome = finish_load(world, &mut saves.handlers, &slot, primary, backup);
            let (result, recovered) = match outcome {
                Ok(recovered) => (Ok(()), recovered),
                Err(error) => (Err(error), false),
            };
            match &result {
                Ok(()) => saves.last_error = None,
                Err(error) => {
                    error!("Loading {slot:?} failed: {error}");
                    saves.last_error = Some(error.clone());
                }
            }
            world.write_message(LoadCompleted {
                slot,
                result,
                recovered,
            });
        }
    });
}

fn report_save(
    world: &mut World,
    last_error: &mut Option<SaveError>,
    slot: SaveSlot,
    result: Result<(), SaveError>,
) {
    match &result {
        Ok(()) => *last_error = None,
        Err(error) => {
            error!("Saving {slot:?} failed: {error}");
            *last_error = Some(error.clone());
        }
    }
    world.write_message(SaveCompleted { slot, result });
}

fn finish_load(
    world: &mut World,
    handlers: &mut [Box<dyn SaveHandler>],
    slot: &SaveSlot,
    primary: ReadResult,
    backup: ReadResult,
) -> Result<bool, SaveError> {
    let primary_result = match primary? {
        Some(bytes) => decode_file(handlers, &bytes),
        None => Err(SaveError::Empty(slot.clone())),
    };
    let (decoded, recovered) = match primary_result {
        Ok(decoded) => (decoded, false),
        Err(primary_error) => match backup.ok().flatten() {
            Some(bytes) => match decode_file(handlers, &bytes) {
                Ok(decoded) => {
                    warn!("Save {slot:?} was unusable ({primary_error}); recovered from backup");
                    (decoded, true)
                }
                Err(_) => return Err(primary_error),
            },
            None => return Err(primary_error),
        },
    };

    // Every entry decoded and migrated successfully; only now is the session modified.
    for (index, value) in decoded {
        handlers[index].restore(world, value)?;
    }
    world.flush();
    for handler in handlers.iter_mut() {
        handler.link(world)?;
    }
    Ok(recovered)
}

/// Installs the save service.
#[derive(Default)]
pub struct KitSavePlugin {
    /// Automatic save behavior.
    pub autosave: AutosavePolicy,
}

impl Plugin for KitSavePlugin {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        ensure_storage(app);
        app.add_message::<AppLifecycle>()
            .add_message::<SaveCompleted>()
            .add_message::<LoadCompleted>()
            .init_resource::<SaveIds>()
            .insert_resource(Saves {
                autosave: self.autosave.clone(),
                ..default()
            })
            .register_type::<SaveId>()
            .add_systems(Last, process_saves);
    }
}
