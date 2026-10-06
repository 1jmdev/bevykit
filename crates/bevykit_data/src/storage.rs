//! Platform storage for settings and saves.
//!
//! [`KitStorage`] stores named byte records through a [`StorageBackend`]. The default backend
//! writes files in the platform's data directory, replacing them atomically and keeping the
//! previous version as a backup. Browsers use `localStorage`. Games can install their own
//! backend, for example a cloud-synchronized one.
//!
//! Writes go through [`KitStorage::write`], which runs them on the IO thread pool and orders
//! them per record: an older request never overwrites a newer one, even if it finishes later.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{IoTaskPool, Task};
use thiserror::Error;

/// A storage failure.
#[derive(Error, Debug, Clone)]
pub enum StorageError {
    /// The underlying IO operation failed.
    #[error("storage IO failed for `{key}`: {message}")]
    Io {
        /// The record.
        key: String,
        /// The platform error.
        message: String,
    },
    /// The backend cannot be used on this platform.
    #[error("storage is unavailable: {0}")]
    Unavailable(String),
    /// The record name contains characters that are not allowed.
    #[error("invalid record name `{0}`")]
    InvalidKey(String),
}

/// A place where named byte records are kept.
pub trait StorageBackend: Send + Sync + 'static {
    /// Reads a record, or `None` if it does not exist.
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Reads the backup of a record kept by the previous write, if the backend keeps one.
    fn read_backup(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;

    /// Replaces a record. Backends should make the replacement atomic where possible.
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError>;

    /// Deletes a record and its backup.
    fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Lists records whose names start with `prefix`.
    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;
}

fn validate_key(key: &str) -> Result<(), StorageError> {
    let valid = !key.is_empty()
        && !key.starts_with('.')
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character));
    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidKey(key.to_string()))
    }
}

/// Stores records as files in a directory.
///
/// A write goes to a temporary file that is flushed to disk, the current file becomes the
/// backup, and the temporary file is renamed into place.
#[derive(Clone, Debug)]
pub struct FileStorage {
    root: PathBuf,
}

impl FileStorage {
    /// Stores records in the given directory, creating it when first written.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Stores records in the platform data directory for the named application.
    ///
    /// Returns `None` when the platform does not report a data directory.
    pub fn for_application(application: &str) -> Option<Self> {
        platform_data_directory(application).map(Self::new)
    }

    /// Returns the directory holding the records.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    fn path(&self, key: &str) -> Result<PathBuf, StorageError> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }

    fn io_error(key: &str, error: std::io::Error) -> StorageError {
        StorageError::Io {
            key: key.to_string(),
            message: error.to_string(),
        }
    }

    fn read_path(key: &str, path: PathBuf) -> Result<Option<Vec<u8>>, StorageError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Self::io_error(key, error)),
        }
    }
}

impl StorageBackend for FileStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Self::read_path(key, self.path(key)?)
    }

    fn read_backup(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Self::read_path(key, self.path(&format!("{key}.bak"))?)
    }

    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        use std::io::Write;

        let target = self.path(key)?;
        let temporary = self.path(&format!("{key}.tmp"))?;
        let backup = self.path(&format!("{key}.bak"))?;
        let error = |error| Self::io_error(key, error);

        std::fs::create_dir_all(&self.root).map_err(error)?;
        {
            let mut file = std::fs::File::create(&temporary).map_err(error)?;
            file.write_all(bytes).map_err(error)?;
            file.sync_all().map_err(error)?;
        }
        if target.exists() {
            std::fs::rename(&target, &backup).map_err(error)?;
        }
        std::fs::rename(&temporary, &target).map_err(error)
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        for path in [self.path(key)?, self.path(&format!("{key}.bak"))?] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(Self::io_error(key, error)),
            }
        }
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(Self::io_error(prefix, error)),
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| {
                name.starts_with(prefix) && !name.ends_with(".bak") && !name.ends_with(".tmp")
            })
            .collect();
        names.sort();
        Ok(names)
    }
}

/// Keeps records in memory. Useful for tests and for platforms without persistent storage.
#[derive(Clone, Debug, Default)]
pub struct MemoryStorage {
    records: Arc<Mutex<HashMap<String, (Vec<u8>, Option<Vec<u8>>)>>>,
}

impl StorageBackend for MemoryStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let records = self.records.lock().expect("storage lock poisoned");
        Ok(records.get(key).map(|(current, _)| current.clone()))
    }

    fn read_backup(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let records = self.records.lock().expect("storage lock poisoned");
        Ok(records.get(key).and_then(|(_, backup)| backup.clone()))
    }

    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        validate_key(key)?;
        let mut records = self.records.lock().expect("storage lock poisoned");
        let backup = records.remove(key).map(|(current, _)| current);
        records.insert(key.to_string(), (bytes.to_vec(), backup));
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.records
            .lock()
            .expect("storage lock poisoned")
            .remove(key);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let records = self.records.lock().expect("storage lock poisoned");
        let mut names: Vec<String> = records
            .keys()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect();
        names.sort();
        Ok(names)
    }
}

/// Stores records in the browser's `localStorage`. Records must be valid UTF-8.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug)]
pub struct WebStorage {
    prefix: String,
}

#[cfg(target_arch = "wasm32")]
impl WebStorage {
    /// Stores records under keys prefixed with the application name.
    pub fn new(application: &str) -> Self {
        Self {
            prefix: format!("{application}/"),
        }
    }

    fn storage(&self) -> Result<web_sys::Storage, StorageError> {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .ok_or_else(|| StorageError::Unavailable("localStorage is not available".into()))
    }

    fn error(key: &str) -> impl Fn(wasm_bindgen::JsValue) -> StorageError + '_ {
        move |value| StorageError::Io {
            key: key.to_string(),
            message: format!("{value:?}"),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl StorageBackend for WebStorage {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let value = self
            .storage()?
            .get_item(&format!("{}{key}", self.prefix))
            .map_err(Self::error(key))?;
        Ok(value.map(String::into_bytes))
    }

    fn read_backup(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.read(&format!("{key}.bak"))
    }

    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        validate_key(key)?;
        let text = std::str::from_utf8(bytes).map_err(|error| StorageError::Io {
            key: key.to_string(),
            message: error.to_string(),
        })?;
        let storage = self.storage()?;
        let full = format!("{}{key}", self.prefix);
        if let Some(previous) = storage.get_item(&full).map_err(Self::error(key))? {
            storage
                .set_item(&format!("{full}.bak"), &previous)
                .map_err(Self::error(key))?;
        }
        storage.set_item(&full, text).map_err(Self::error(key))
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        let storage = self.storage()?;
        let full = format!("{}{key}", self.prefix);
        storage.remove_item(&full).map_err(Self::error(key))?;
        storage
            .remove_item(&format!("{full}.bak"))
            .map_err(Self::error(key))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let storage = self.storage()?;
        let length = storage.length().map_err(Self::error(prefix))?;
        let mut names = Vec::new();
        for index in 0..length {
            if let Ok(Some(name)) = storage.key(index)
                && let Some(name) = name.strip_prefix(&self.prefix)
                && name.starts_with(prefix)
                && !name.ends_with(".bak")
            {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn platform_data_directory(application: &str) -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", application).map(|dirs| dirs.data_dir().to_path_buf())
}

#[cfg(target_os = "android")]
fn platform_data_directory(application: &str) -> Option<PathBuf> {
    let _ = application;
    bevy::android::ANDROID_APP
        .get()
        .and_then(|app| app.internal_data_path())
}

#[cfg(target_arch = "wasm32")]
fn platform_data_directory(_application: &str) -> Option<PathBuf> {
    None
}

/// The outcome of an asynchronous write.
#[derive(Message, Clone, Debug)]
pub struct StorageWriteFinished {
    /// The record.
    pub key: String,
    /// The sequence number of the request, increasing per record.
    pub sequence: u64,
    /// `Ok` if written, `Err` if failed. Superseded requests report `Ok` without writing.
    pub result: Result<(), StorageError>,
}

#[derive(Default)]
struct WriteOrder {
    requested: HashMap<String, u64>,
    written: Arc<Mutex<HashMap<String, u64>>>,
}

/// The storage used by settings and saves.
#[derive(Resource, Clone)]
pub struct KitStorage {
    backend: Arc<dyn StorageBackend>,
    order: Arc<Mutex<WriteOrder>>,
}

impl KitStorage {
    /// Uses the given backend.
    pub fn new(backend: impl StorageBackend) -> Self {
        Self {
            backend: Arc::new(backend),
            order: Arc::default(),
        }
    }

    /// Uses the platform default for the named application: files in the data directory on
    /// desktop and mobile, `localStorage` in browsers, memory where neither is available.
    pub fn platform_default(application: &str) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::new(WebStorage::new(application))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            match FileStorage::for_application(application) {
                Some(storage) => Self::new(storage),
                None => {
                    warn!("No platform data directory; settings and saves will not persist");
                    Self::new(MemoryStorage::default())
                }
            }
        }
    }

    /// Returns the backend.
    pub fn backend(&self) -> &dyn StorageBackend {
        self.backend.as_ref()
    }

    /// Reads a record synchronously.
    pub fn read(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.backend.read(key)
    }

    /// Writes a record synchronously, bypassing the queue. Use when the platform may suspend
    /// the application before an asynchronous write could finish.
    pub fn write_blocking(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let sequence = self.next_sequence(key);
        let written = self.order.lock().expect("storage lock poisoned").written.clone();
        Self::ordered_write(self.backend.as_ref(), &written, key, sequence, bytes)
    }

    /// Queues an asynchronous write and returns its sequence number.
    pub fn write(&self, key: &str, bytes: Vec<u8>) -> PendingWrite {
        let sequence = self.next_sequence(key);
        let written = self.order.lock().expect("storage lock poisoned").written.clone();
        let backend = self.backend.clone();
        let owned_key = key.to_string();
        let task = IoTaskPool::get().spawn(async move {
            let result =
                Self::ordered_write(backend.as_ref(), &written, &owned_key, sequence, &bytes);
            StorageWriteFinished {
                key: owned_key,
                sequence,
                result,
            }
        });
        PendingWrite { task, sequence }
    }

    fn next_sequence(&self, key: &str) -> u64 {
        let mut order = self.order.lock().expect("storage lock poisoned");
        let sequence = order.requested.entry(key.to_string()).or_insert(0);
        *sequence += 1;
        *sequence
    }

    fn ordered_write(
        backend: &dyn StorageBackend,
        written: &Mutex<HashMap<String, u64>>,
        key: &str,
        sequence: u64,
        bytes: &[u8],
    ) -> Result<(), StorageError> {
        // Holding the lock across the write serializes writes, so the check and the write
        // happen atomically with respect to other requests.
        let mut written = written.lock().expect("storage lock poisoned");
        if written.get(key).is_some_and(|latest| *latest >= sequence) {
            return Ok(());
        }
        backend.write(key, bytes)?;
        written.insert(key.to_string(), sequence);
        Ok(())
    }
}

/// An asynchronous write in progress.
pub struct PendingWrite {
    task: Task<StorageWriteFinished>,
    sequence: u64,
}

impl PendingWrite {
    /// Returns the sequence number of the write.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the result once the write finishes.
    pub fn poll(&mut self) -> Option<StorageWriteFinished> {
        check_ready(&mut self.task)
    }

    /// Waits for the write to finish.
    pub fn wait(self) -> StorageWriteFinished {
        bevy::tasks::block_on(self.task)
    }
}

/// Inserts platform storage for the application unless the game already provided one.
pub fn ensure_storage(app: &mut App) {
    if app.world().contains_resource::<KitStorage>() {
        return;
    }
    let application = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem()?.to_str().map(str::to_string))
        .unwrap_or_else(|| "bevykit".to_string());
    app.insert_resource(KitStorage::platform_default(&application));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_storage_keeps_backup_and_lists_records() {
        let root = std::env::temp_dir().join(format!("bevykit-storage-{}", std::process::id()));
        let storage = FileStorage::new(&root);
        storage.write("slot-1.json", b"first").unwrap();
        storage.write("slot-1.json", b"second").unwrap();
        storage.write("slot-2.json", b"other").unwrap();

        assert_eq!(storage.read("slot-1.json").unwrap().unwrap(), b"second");
        assert_eq!(storage.read_backup("slot-1.json").unwrap().unwrap(), b"first");
        assert_eq!(
            storage.list("slot-").unwrap(),
            vec!["slot-1.json".to_string(), "slot-2.json".to_string()]
        );
        assert!(storage.write("../escape", b"no").is_err());

        storage.delete("slot-1.json").unwrap();
        assert!(storage.read("slot-1.json").unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn older_write_never_overwrites_newer() {
        let storage = KitStorage::new(MemoryStorage::default());
        let first_sequence = storage.next_sequence("record");
        let written = storage.order.lock().unwrap().written.clone();
        storage.write_blocking("record", b"newer").unwrap();
        KitStorage::ordered_write(
            storage.backend(),
            &written,
            "record",
            first_sequence,
            b"older",
        )
        .unwrap();
        assert_eq!(storage.read("record").unwrap().unwrap(), b"newer");
    }
}
