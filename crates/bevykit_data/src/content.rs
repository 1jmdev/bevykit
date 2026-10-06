//! Typed, validated content definitions loaded from data files.
//!
//! A definition is any deserializable type deriving [`ContentDefinition`]. Definitions are
//! read from RON, JSON, or TOML files; each file holds one definition or a list of them.
//!
//! ```ignore
//! #[derive(Deserialize, ContentDefinition)]
//! struct ItemDefinition {
//!     id: ContentId<Self>,
//!     name: LocalizedKey,
//!     max_stack: u32,
//!     #[content(reference)]
//!     upgrades_to: Option<ContentId<ItemDefinition>>,
//! }
//!
//! fn configure_content(mut content: ResMut<ContentRegistry>) {
//!     content
//!         .register::<ItemDefinition>()
//!         .load_directory("data/items")
//!         .validate(|item| {
//!             require!(item.max_stack > 0, "max_stack must be positive");
//!         });
//! }
//!
//! fn inspect_item(items: Res<Definitions<ItemDefinition>>) {
//!     let item = items.get("healing_item");
//! }
//! ```
//!
//! Every registered kind is parsed and validated together: duplicate IDs, missing references,
//! failed validators, and malformed files are all reported with the file, definition, and field
//! where possible. Only a fully valid set is published, so systems always see a consistent
//! revision. A reload that fails keeps the previous revision.

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::io::{AssetSourceId, ErasedAssetReader};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::futures_lite::StreamExt;
use bevy::tasks::{IoTaskPool, Task};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use bevykit_macros::ContentDefinition;

/// A typed identifier of a content definition.
pub struct ContentId<T> {
    id: Cow<'static, str>,
    marker: PhantomData<fn() -> T>,
}

impl<T> ContentId<T> {
    /// Creates an identifier.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self {
            id: id.into(),
            marker: PhantomData,
        }
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

impl<T> Clone for ContentId<T> {
    fn clone(&self) -> Self {
        Self::new(self.id.clone())
    }
}

impl<T> PartialEq for ContentId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for ContentId<T> {}

impl<T> Hash for ContentId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for ContentId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContentId({})", self.id)
    }
}

impl<T> fmt::Display for ContentId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.id)
    }
}

impl<T> AsRef<str> for ContentId<T> {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

impl<T> Serialize for ContentId<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.id)
    }
}

impl<'de, T> Deserialize<'de> for ContentId<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// A reference from one definition to another, collected for validation.
#[derive(Clone, Debug)]
pub struct ContentReference {
    /// The field holding the reference.
    pub field: &'static str,
    /// The referenced definition type.
    pub kind: TypeId,
    /// The referenced definition type's name.
    pub kind_name: &'static str,
    /// The referenced identifier.
    pub id: String,
}

/// Collects the references held by a field. Implemented for [`ContentId`], and for `Option`
/// and `Vec` of collectable values.
pub trait CollectReferences {
    /// Appends the references held by this value.
    fn collect_references(&self, field: &'static str, references: &mut Vec<ContentReference>);
}

impl<T: ContentDefinition> CollectReferences for ContentId<T> {
    fn collect_references(&self, field: &'static str, references: &mut Vec<ContentReference>) {
        references.push(ContentReference {
            field,
            kind: TypeId::of::<T>(),
            kind_name: T::KIND,
            id: self.id.to_string(),
        });
    }
}

impl<T: CollectReferences> CollectReferences for Option<T> {
    fn collect_references(&self, field: &'static str, references: &mut Vec<ContentReference>) {
        if let Some(value) = self {
            value.collect_references(field, references);
        }
    }
}

impl<T: CollectReferences> CollectReferences for Vec<T> {
    fn collect_references(&self, field: &'static str, references: &mut Vec<ContentReference>) {
        for value in self {
            value.collect_references(field, references);
        }
    }
}

/// A game-defined content type. Usually implemented with `#[derive(ContentDefinition)]`.
pub trait ContentDefinition: DeserializeOwned + Send + Sync + 'static {
    /// A readable name of the kind, used in error messages.
    const KIND: &'static str;

    /// Returns the definition's identifier.
    fn id(&self) -> &ContentId<Self>;

    /// Appends the references this definition holds to other definitions.
    fn references(&self, references: &mut Vec<ContentReference>) {
        let _ = references;
    }
}

/// A content problem.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentError {
    /// The definition kind.
    pub kind: &'static str,
    /// The file, if known.
    pub file: Option<String>,
    /// The definition identifier, if known.
    pub id: Option<String>,
    /// The field, if known.
    pub field: Option<String>,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}]", self.kind)?;
        if let Some(file) = &self.file {
            write!(formatter, " {file}")?;
        }
        if let Some(id) = &self.id {
            write!(formatter, " `{id}`")?;
        }
        if let Some(field) = &self.field {
            write!(formatter, " .{field}")?;
        }
        write!(formatter, ": {}", self.message)
    }
}

thread_local! {
    static VALIDATION_ISSUES: RefCell<Option<Vec<(Option<String>, String)>>> =
        const { RefCell::new(None) };
}

/// Records a validation failure. Used by [`require!`](crate::require); only meaningful inside
/// a validator registered with [`KindBuilder::validate`].
pub fn report_issue(field: Option<&str>, message: String) {
    VALIDATION_ISSUES.with(|issues| {
        if let Some(issues) = issues.borrow_mut().as_mut() {
            issues.push((field.map(str::to_string), message));
        }
    });
}

/// Inside a content validator, reports a failure when `condition` is false.
///
/// ```ignore
/// require!(item.max_stack > 0, "max_stack must be positive");
/// require!(item.price <= 1000, field = "price", "price {} is too high", item.price);
/// ```
#[macro_export]
macro_rules! require {
    ($condition:expr, field = $field:expr, $($message:tt)+) => {
        if !$condition {
            $crate::content::report_issue(Some($field), format!($($message)+));
        }
    };
    ($condition:expr, $($message:tt)+) => {
        if !$condition {
            $crate::content::report_issue(None, format!($($message)+));
        }
    };
}

/// Every definition of type `T` in the current revision.
#[derive(Resource)]
pub struct Definitions<T: ContentDefinition> {
    definitions: HashMap<String, Arc<T>>,
    order: Vec<String>,
    revision: u64,
}

impl<T: ContentDefinition> Definitions<T> {
    /// Returns a definition by identifier.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&T> {
        self.definitions.get(id.as_ref()).map(Arc::as_ref)
    }

    /// Returns a shared pointer to a definition, for keeping it beyond a reload.
    pub fn get_shared(&self, id: impl AsRef<str>) -> Option<Arc<T>> {
        self.definitions.get(id.as_ref()).cloned()
    }

    /// Returns `true` if the identifier exists.
    pub fn contains(&self, id: impl AsRef<str>) -> bool {
        self.definitions.contains_key(id.as_ref())
    }

    /// Iterates definitions in file order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.order
            .iter()
            .filter_map(|id| self.definitions.get(id).map(Arc::as_ref))
    }

    /// Returns the number of definitions.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Returns `true` if there are no definitions.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Returns the revision these definitions belong to.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Sent when a new content revision is published.
#[derive(Message, Clone, Copy, Debug)]
pub struct ContentPublished {
    /// The new revision.
    pub revision: u64,
}

/// Sent when loading or reloading content fails. The previous revision stays active.
#[derive(Message, Clone, Debug)]
pub struct ContentRejected {
    /// Every problem found.
    pub errors: Vec<ContentError>,
}

#[derive(Clone, Debug)]
enum ContentSource {
    Directory(PathBuf),
    File(PathBuf),
}

type Validator<T> = Arc<dyn Fn(&T) + Send + Sync>;

trait ErasedKind: Send + Sync {
    fn kind(&self) -> &'static str;
    fn definition_type(&self) -> TypeId;
    fn sources(&self) -> &[ContentSource];
    fn parse(&self, files: &[(String, Vec<u8>)], errors: &mut Vec<ContentError>) -> Box<dyn ParsedKind>;
    fn clone_box(&self) -> Box<dyn ErasedKind>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

trait ParsedKind: Send {
    fn ids(&self) -> HashSet<String>;
    fn check(&self, known: &HashMap<TypeId, HashSet<String>>, errors: &mut Vec<ContentError>);
    fn publish(self: Box<Self>, world: &mut World, revision: u64);
}

struct Kind<T: ContentDefinition> {
    sources: Vec<ContentSource>,
    validators: Vec<Validator<T>>,
}

impl<T: ContentDefinition> Clone for Kind<T> {
    fn clone(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            validators: self.validators.clone(),
        }
    }
}

struct Parsed<T: ContentDefinition> {
    definitions: Vec<(String, T)>,
    validators: Vec<Validator<T>>,
}

fn parse_document<T: DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<Vec<T>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let extension = Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    match extension {
        "ron" => ron::from_str::<Vec<T>>(text)
            .or_else(|_| ron::from_str::<T>(text).map(|single| vec![single]))
            .map_err(|error| error.to_string()),
        "json" => serde_json::from_str::<Vec<T>>(text)
            .or_else(|_| serde_json::from_str::<T>(text).map(|single| vec![single]))
            .map_err(|error| error.to_string()),
        "toml" => {
            #[derive(Deserialize)]
            struct Document<T> {
                definitions: Vec<T>,
            }
            toml::from_str::<Document<T>>(text)
                .map(|document| document.definitions)
                .or_else(|_| toml::from_str::<T>(text).map(|single| vec![single]))
                .map_err(|error| error.to_string())
        }
        other => Err(format!("unsupported content format `{other}`")),
    }
}

impl<T: ContentDefinition> ErasedKind for Kind<T> {
    fn kind(&self) -> &'static str {
        T::KIND
    }

    fn definition_type(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn sources(&self) -> &[ContentSource] {
        &self.sources
    }

    fn parse(&self, files: &[(String, Vec<u8>)], errors: &mut Vec<ContentError>) -> Box<dyn ParsedKind> {
        let mut definitions: Vec<(String, T)> = Vec::new();
        let mut seen: HashMap<String, String> = HashMap::default();
        for (file, bytes) in files {
            match parse_document::<T>(file, bytes) {
                Ok(parsed) => {
                    for definition in parsed {
                        let id = definition.id().as_str().to_string();
                        if let Some(previous) = seen.insert(id.clone(), file.clone()) {
                            errors.push(ContentError {
                                kind: T::KIND,
                                file: Some(file.clone()),
                                id: Some(id),
                                field: Some("id".to_string()),
                                message: format!("duplicate identifier, first defined in {previous}"),
                            });
                            continue;
                        }
                        definitions.push((file.clone(), definition));
                    }
                }
                Err(message) => errors.push(ContentError {
                    kind: T::KIND,
                    file: Some(file.clone()),
                    id: None,
                    field: None,
                    message,
                }),
            }
        }
        Box::new(Parsed {
            definitions,
            validators: self.validators.clone(),
        })
    }

    fn clone_box(&self) -> Box<dyn ErasedKind> {
        Box::new(self.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<T: ContentDefinition> ParsedKind for Parsed<T> {
    fn ids(&self) -> HashSet<String> {
        self.definitions
            .iter()
            .map(|(_, definition)| definition.id().as_str().to_string())
            .collect()
    }

    fn check(&self, known: &HashMap<TypeId, HashSet<String>>, errors: &mut Vec<ContentError>) {
        for (file, definition) in &self.definitions {
            let id = definition.id().as_str().to_string();
            let mut references = Vec::new();
            definition.references(&mut references);
            for reference in references {
                let message = match known.get(&reference.kind) {
                    None => Some(format!(
                        "references kind `{}`, which is not registered",
                        reference.kind_name
                    )),
                    Some(ids) if !ids.contains(&reference.id) => Some(format!(
                        "references missing {} `{}`",
                        reference.kind_name, reference.id
                    )),
                    Some(_) => None,
                };
                if let Some(message) = message {
                    errors.push(ContentError {
                        kind: T::KIND,
                        file: Some(file.clone()),
                        id: Some(id.clone()),
                        field: Some(reference.field.to_string()),
                        message,
                    });
                }
            }

            VALIDATION_ISSUES.with(|issues| *issues.borrow_mut() = Some(Vec::new()));
            for validator in &self.validators {
                validator(definition);
            }
            let issues = VALIDATION_ISSUES.with(|issues| issues.borrow_mut().take());
            for (field, message) in issues.unwrap_or_default() {
                errors.push(ContentError {
                    kind: T::KIND,
                    file: Some(file.clone()),
                    id: Some(id.clone()),
                    field,
                    message,
                });
            }
        }
    }

    fn publish(self: Box<Self>, world: &mut World, revision: u64) {
        let mut definitions = HashMap::default();
        let mut order = Vec::with_capacity(self.definitions.len());
        for (_, definition) in self.definitions {
            let id = definition.id().as_str().to_string();
            order.push(id.clone());
            definitions.insert(id, Arc::new(definition));
        }
        world.insert_resource(Definitions::<T> {
            definitions,
            order,
            revision,
        });
    }
}

/// Builder for a registered kind. Returned by [`ContentRegistry::register`].
pub struct KindBuilder<'a, T: ContentDefinition> {
    kind: &'a mut Kind<T>,
}

impl<T: ContentDefinition> KindBuilder<'_, T> {
    /// Loads every `.ron`, `.json`, and `.toml` file in an asset directory, recursively.
    ///
    /// Directory listing is unavailable on Android and the web; use
    /// [`load_file`](Self::load_file) there.
    pub fn load_directory(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.kind.sources.push(ContentSource::Directory(path.into()));
        self
    }

    /// Loads a single asset file.
    pub fn load_file(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.kind.sources.push(ContentSource::File(path.into()));
        self
    }

    /// Adds a validator. Report failures with [`require!`](crate::require).
    pub fn validate(&mut self, validator: impl Fn(&T) + Send + Sync + 'static) -> &mut Self {
        self.kind.validators.push(Arc::new(validator));
        self
    }
}

/// The state of the content registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ContentStatus {
    /// Nothing has been loaded yet.
    #[default]
    Unloaded,
    /// Files are being read and validated.
    Loading,
    /// A valid revision is published.
    Ready,
    /// The most recent load failed. A previous revision may still be active.
    Rejected(Vec<ContentError>),
}

struct LoadOutcome {
    fingerprint: u64,
    result: Result<Vec<Box<dyn ParsedKind>>, Vec<ContentError>>,
}

/// Registers content kinds and publishes validated revisions.
#[derive(Resource, Default)]
pub struct ContentRegistry {
    kinds: Vec<Box<dyn ErasedKind>>,
    dirty: bool,
    loading: Option<Task<LoadOutcome>>,
    status: ContentStatus,
    revision: u64,
    fingerprint: Option<u64>,
    /// Re-read content files at this interval and publish changes. Intended for development.
    pub watch_interval: Option<Duration>,
    since_watch: Duration,
}

impl ContentRegistry {
    /// Registers a kind, or returns the existing registration.
    pub fn register<T: ContentDefinition>(&mut self) -> KindBuilder<'_, T> {
        self.dirty = true;
        let index = match self
            .kinds
            .iter()
            .position(|kind| kind.definition_type() == TypeId::of::<T>())
        {
            Some(index) => index,
            None => {
                self.kinds.push(Box::new(Kind::<T> {
                    sources: Vec::new(),
                    validators: Vec::new(),
                }));
                self.kinds.len() - 1
            }
        };
        let kind = self.kinds[index]
            .as_any_mut()
            .downcast_mut::<Kind<T>>()
            .expect("registered kinds match their type");
        KindBuilder { kind }
    }

    /// Re-reads every file and publishes a new revision if it is valid.
    pub fn reload(&mut self) {
        self.dirty = true;
        self.fingerprint = None;
    }

    /// Returns the registry status.
    pub fn status(&self) -> &ContentStatus {
        &self.status
    }

    /// Returns the current revision. Zero until the first publication.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn snapshot_kinds(&self) -> Vec<Box<dyn ErasedKind>> {
        let mut kinds: Vec<Box<dyn ErasedKind>> =
            self.kinds.iter().map(|kind| kind.clone_box()).collect();
        kinds.sort_by_key(|kind| kind.kind());
        kinds
    }
}

async fn read_files(
    reader: &dyn ErasedAssetReader,
    sources: &[ContentSource],
    errors: &mut Vec<ContentError>,
    kind: &'static str,
) -> Vec<(String, Vec<u8>)> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut directories: Vec<PathBuf> = Vec::new();
    for source in sources {
        match source {
            ContentSource::File(path) => paths.push(path.clone()),
            ContentSource::Directory(path) => directories.push(path.clone()),
        }
    }
    while let Some(directory) = directories.pop() {
        match reader.read_directory(&directory).await {
            Ok(mut entries) => {
                let mut listed = Vec::new();
                while let Some(entry) = entries.next().await {
                    listed.push(entry);
                }
                for entry in listed {
                    if reader.is_directory(&entry).await.unwrap_or(false) {
                        directories.push(entry);
                    } else if matches!(
                        entry.extension().and_then(|extension| extension.to_str()),
                        Some("ron" | "json" | "toml")
                    ) {
                        paths.push(entry);
                    }
                }
            }
            Err(error) => errors.push(ContentError {
                kind,
                file: Some(directory.display().to_string()),
                id: None,
                field: None,
                message: format!("cannot list directory: {error}"),
            }),
        }
    }
    paths.sort();
    paths.dedup();

    let mut files = Vec::new();
    for path in paths {
        let display = path.display().to_string();
        let bytes = match reader.read(&path).await {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                match file.read_to_end(&mut bytes).await {
                    Ok(_) => Ok(bytes),
                    Err(error) => Err(error.to_string()),
                }
            }
            Err(error) => Err(error.to_string()),
        };
        match bytes {
            Ok(bytes) => files.push((display, bytes)),
            Err(message) => errors.push(ContentError {
                kind,
                file: Some(display),
                id: None,
                field: None,
                message,
            }),
        }
    }
    files
}

fn fingerprint(files: &[(&'static str, Vec<(String, Vec<u8>)>)]) -> u64 {
    let mut hasher = bevykit_core::key::StableHasher::new();
    for (kind, kind_files) in files {
        kind.hash(&mut hasher);
        for (path, bytes) in kind_files {
            path.hash(&mut hasher);
            bytes.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn start_load(server: &AssetServer, kinds: Vec<Box<dyn ErasedKind>>) -> Task<LoadOutcome> {
    let server = server.clone();
    IoTaskPool::get().spawn(async move {
        let mut errors = Vec::new();
        let Ok(source) = server.get_source(AssetSourceId::Default) else {
            return LoadOutcome {
                fingerprint: 0,
                result: Err(vec![ContentError {
                    kind: "content",
                    file: None,
                    id: None,
                    field: None,
                    message: "the default asset source is missing".to_string(),
                }]),
            };
        };
        let reader = source.reader();
        let mut all_files = Vec::new();
        for kind in &kinds {
            let files = read_files(reader, kind.sources(), &mut errors, kind.kind()).await;
            all_files.push((kind.kind(), files));
        }

        let parsed: Vec<Box<dyn ParsedKind>> = kinds
            .iter()
            .zip(&all_files)
            .map(|(kind, (_, files))| kind.parse(files, &mut errors))
            .collect();
        let known: HashMap<TypeId, HashSet<String>> = kinds
            .iter()
            .zip(&parsed)
            .map(|(kind, parsed)| (kind.definition_type(), parsed.ids()))
            .collect();
        for kind in &parsed {
            kind.check(&known, &mut errors);
        }

        LoadOutcome {
            fingerprint: fingerprint(&all_files),
            result: if errors.is_empty() {
                Ok(parsed)
            } else {
                Err(errors)
            },
        }
    })
}

pub(crate) fn process_content(world: &mut World) {
    let delta = world.resource::<Time<Real>>().delta();
    world.resource_scope(|world, mut registry: Mut<ContentRegistry>| {
        if let Some(interval) = registry.watch_interval {
            registry.since_watch += delta;
            if registry.since_watch >= interval && registry.loading.is_none() {
                registry.since_watch = Duration::ZERO;
                registry.dirty = true;
            }
        }

        if registry.dirty && registry.loading.is_none() {
            registry.dirty = false;
            let kinds = registry.snapshot_kinds();
            if !kinds.is_empty() {
                let server = world.resource::<AssetServer>().clone();
                registry.loading = Some(start_load(&server, kinds));
                if registry.revision == 0 {
                    registry.status = ContentStatus::Loading;
                }
            }
        }

        let Some(outcome) = registry.loading.as_mut().and_then(check_ready) else {
            return;
        };
        registry.loading = None;
        if registry.fingerprint == Some(outcome.fingerprint) {
            return;
        }
        match outcome.result {
            Ok(parsed) => {
                registry.fingerprint = Some(outcome.fingerprint);
                registry.revision += 1;
                let revision = registry.revision;
                for kind in parsed {
                    kind.publish(world, revision);
                }
                registry.status = ContentStatus::Ready;
                world.write_message(ContentPublished { revision });
            }
            Err(errors) => {
                registry.fingerprint = Some(outcome.fingerprint);
                for error in &errors {
                    error!("Content rejected: {error}");
                }
                registry.status = ContentStatus::Rejected(errors.clone());
                world.write_message(ContentRejected { errors });
            }
        }
    });
}
