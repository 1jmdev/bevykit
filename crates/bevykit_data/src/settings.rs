//! Typed, validated, persisted settings that apply immediately.
//!
//! Settings are a game-defined struct deriving [`Settings`]. They load synchronously when the
//! plugin is added, so the first frame already uses the player's language and volume. Changes
//! are sanitized, applied to the live values (triggering change detection), and persisted
//! asynchronously.
//!
//! Screens that let the player try a value before confirming it use an edit transaction:
//!
//! ```ignore
//! let edit = settings.begin_edit();
//! settings.preview(edit, |values| values.music_volume = 0.5);
//! settings.commit(edit); // or settings.cancel(edit) to restore the previous values
//! ```

use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::{Deref, RangeInclusive};

use bevy::prelude::*;
use bevy::window::AppLifecycle;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::storage::{KitStorage, PendingWrite, StorageError, ensure_storage};

pub use bevykit_macros::Settings;

/// A problem found while sanitizing settings. The offending value has already been corrected.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingIssue {
    /// The field that was corrected.
    pub field: &'static str,
    /// What was wrong.
    pub message: String,
}

/// A game-defined settings struct. Usually implemented with `#[derive(Settings)]`.
///
/// Fields annotated `#[setting(range = a..=b)]` are clamped into range, and fields annotated
/// `#[setting(validate = path)]` are passed to `fn(&mut T) -> Result<(), String>`.
/// The container attribute `#[settings(key = "name")]` chooses the storage record.
pub trait Settings: Serialize + DeserializeOwned + Default + Clone + Send + Sync + 'static {
    /// The storage record holding these settings.
    const STORAGE_KEY: &'static str;

    /// Corrects invalid values and reports what was changed.
    fn sanitize(&mut self) -> Vec<SettingIssue>;
}

/// Clamps a value into range, recording an issue when it was outside. Used by the derive.
pub fn clamp_to_range<T: PartialOrd + Copy + std::fmt::Debug>(
    value: &mut T,
    range: RangeInclusive<T>,
    field: &'static str,
    issues: &mut Vec<SettingIssue>,
) {
    let (start, end) = (*range.start(), *range.end());
    // Incomparable values, such as NaN, are replaced by the lower bound.
    let corrected = match ((*value).partial_cmp(&start), (*value).partial_cmp(&end)) {
        (Some(Ordering::Less) | None, _) => start,
        (_, Some(Ordering::Greater)) => end,
        _ => return,
    };
    issues.push(SettingIssue {
        field,
        message: format!("{value:?} is outside {start:?}..={end:?}"),
    });
    *value = corrected;
}

/// Applies a custom validator, recording an issue when it fails. Used by the derive.
pub fn apply_validator<T>(
    value: &mut T,
    validator: impl FnOnce(&mut T) -> Result<(), String>,
    field: &'static str,
    issues: &mut Vec<SettingIssue>,
) {
    if let Err(message) = validator(value) {
        issues.push(SettingIssue { field, message });
    }
}

/// Identifies an edit transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SettingsEdit(u64);

/// Whether the settings on disk match the committed values.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PersistStatus {
    /// Stored values match the committed values.
    #[default]
    Saved,
    /// A write is pending or in progress.
    Saving,
    /// The most recent write failed.
    Failed(String),
}

/// The live settings and their persistence state.
///
/// Dereferences to the live values, so `settings.music_volume` reads the current volume and
/// `settings.is_changed()` reports when any value changed.
#[derive(Resource)]
pub struct SettingsStore<S: Settings> {
    live: S,
    committed: S,
    active_edit: Option<SettingsEdit>,
    next_edit: u64,
    persist_requested: bool,
    status: PersistStatus,
    issues: Vec<SettingIssue>,
}

impl<S: Settings> Deref for SettingsStore<S> {
    type Target = S;

    fn deref(&self) -> &S {
        &self.live
    }
}

impl<S: Settings> SettingsStore<S> {
    fn new(mut values: S) -> Self {
        let issues = values.sanitize();
        Self {
            committed: values.clone(),
            live: values,
            active_edit: None,
            next_edit: 0,
            persist_requested: false,
            status: PersistStatus::Saved,
            issues,
        }
    }

    /// Returns the live values.
    pub fn get(&self) -> &S {
        &self.live
    }

    /// Returns the committed values, which differ from the live values during a preview.
    pub fn committed(&self) -> &S {
        &self.committed
    }

    /// Changes values immediately, commits them, and persists them.
    pub fn set(&mut self, change: impl FnOnce(&mut S)) {
        change(&mut self.live);
        self.issues = self.live.sanitize();
        self.committed = self.live.clone();
        self.active_edit = None;
        self.persist_requested = true;
    }

    /// Restores every value to its default and persists the result.
    pub fn reset(&mut self) {
        self.set(|values| *values = S::default());
    }

    /// Begins an edit transaction. An edit already in progress is cancelled.
    pub fn begin_edit(&mut self) -> SettingsEdit {
        if self.active_edit.is_some() {
            self.live = self.committed.clone();
        }
        self.next_edit += 1;
        let edit = SettingsEdit(self.next_edit);
        self.active_edit = Some(edit);
        edit
    }

    /// Applies a change to the live values without committing it. Ignored if `edit` is not
    /// the active transaction.
    pub fn preview(&mut self, edit: SettingsEdit, change: impl FnOnce(&mut S)) {
        if self.active_edit != Some(edit) {
            warn!("Ignoring a settings preview for an edit that is no longer active");
            return;
        }
        change(&mut self.live);
        self.issues = self.live.sanitize();
    }

    /// Commits and persists the previewed values.
    pub fn commit(&mut self, edit: SettingsEdit) {
        if self.active_edit != Some(edit) {
            return;
        }
        self.active_edit = None;
        self.committed = self.live.clone();
        self.persist_requested = true;
    }

    /// Discards the previewed values and restores the committed ones.
    pub fn cancel(&mut self, edit: SettingsEdit) {
        if self.active_edit != Some(edit) {
            return;
        }
        self.active_edit = None;
        self.live = self.committed.clone();
    }

    /// Returns `true` while an edit transaction is open.
    pub fn is_editing(&self) -> bool {
        self.active_edit.is_some()
    }

    /// Returns the persistence status.
    pub fn status(&self) -> &PersistStatus {
        &self.status
    }

    /// Returns the corrections made by the most recent sanitization.
    pub fn issues(&self) -> &[SettingIssue] {
        &self.issues
    }
}

/// Loads settings, merging stored values over the defaults so that fields added in a newer
/// version of the game keep their default values.
pub fn load_settings<S: Settings>(storage: &KitStorage) -> Result<S, String> {
    let Some(bytes) = storage.read(S::STORAGE_KEY).map_err(|error| error.to_string())? else {
        return Ok(S::default());
    };
    let stored: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let mut merged = serde_json::to_value(S::default()).map_err(|error| error.to_string())?;
    merge_json(&mut merged, stored);
    serde_json::from_value(merged).map_err(|error| error.to_string())
}

/// Recursively overlays `overlay` onto `base`. Objects merge by key; other values replace.
pub fn merge_json(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn encode<S: Settings>(values: &S) -> Result<Vec<u8>, StorageError> {
    serde_json::to_vec_pretty(values).map_err(|error| StorageError::Io {
        key: S::STORAGE_KEY.to_string(),
        message: error.to_string(),
    })
}

fn persist_settings<S: Settings>(
    mut store: ResMut<SettingsStore<S>>,
    storage: Res<KitStorage>,
    mut pending: Local<Vec<PendingWrite>>,
    mut lifecycle: MessageReader<AppLifecycle>,
) {
    let suspending = lifecycle
        .read()
        .any(|event| matches!(event, AppLifecycle::WillSuspend));

    if store.persist_requested {
        store.bypass_change_detection().persist_requested = false;
        match encode(&store.committed) {
            Ok(bytes) if suspending => {
                let result = storage.write_blocking(S::STORAGE_KEY, &bytes);
                store.status = match result {
                    Ok(()) => PersistStatus::Saved,
                    Err(error) => PersistStatus::Failed(error.to_string()),
                };
            }
            Ok(bytes) => {
                pending.push(storage.write(S::STORAGE_KEY, bytes));
                store.bypass_change_detection().status = PersistStatus::Saving;
            }
            Err(error) => store.status = PersistStatus::Failed(error.to_string()),
        }
    }

    pending.retain_mut(|write| match write.poll() {
        Some(finished) => {
            let status = match finished.result {
                Ok(()) => PersistStatus::Saved,
                Err(error) => {
                    error!("Failed to save settings: {error}");
                    PersistStatus::Failed(error.to_string())
                }
            };
            store.bypass_change_detection().status = status;
            false
        }
        None => true,
    });

    if suspending {
        for write in pending.drain(..) {
            let _ = write.wait();
        }
    }
}

/// Loads, sanitizes, and persists settings of type `S`.
pub struct KitSettingsPlugin<S: Settings> {
    marker: PhantomData<S>,
}

impl<S: Settings> Default for KitSettingsPlugin<S> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<S: Settings> Plugin for KitSettingsPlugin<S> {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        ensure_storage(app);
        app.add_message::<AppLifecycle>();

        let storage = app.world().resource::<KitStorage>().clone();
        let values = load_settings::<S>(&storage).unwrap_or_else(|error| {
            warn!(
                "Settings `{}` could not be read ({error}); using defaults",
                S::STORAGE_KEY
            );
            S::default()
        });
        let store = SettingsStore::new(values);
        for issue in store.issues() {
            warn!("Setting `{}` corrected: {}", issue.field, issue.message);
        }
        app.insert_resource(store)
            .add_systems(Last, persist_settings::<S>);
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::storage::MemoryStorage;

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    struct Example {
        volume: f32,
        language: String,
    }

    impl Default for Example {
        fn default() -> Self {
            Self {
                volume: 0.8,
                language: "en".to_string(),
            }
        }
    }

    impl Settings for Example {
        const STORAGE_KEY: &'static str = "example.json";

        fn sanitize(&mut self) -> Vec<SettingIssue> {
            let mut issues = Vec::new();
            clamp_to_range(&mut self.volume, 0.0..=1.0, "volume", &mut issues);
            issues
        }
    }

    #[test]
    fn missing_fields_keep_defaults() {
        let storage = KitStorage::new(MemoryStorage::default());
        storage
            .write_blocking("example.json", br#"{ "volume": 0.25 }"#)
            .unwrap();
        let loaded: Example = load_settings(&storage).unwrap();
        assert_eq!(loaded.volume, 0.25);
        assert_eq!(loaded.language, "en");
    }

    #[test]
    fn preview_and_cancel_restore_committed_values() {
        let mut store = SettingsStore::new(Example::default());
        let edit = store.begin_edit();
        store.preview(edit, |values| values.volume = 3.0);
        assert_eq!(store.volume, 1.0);
        assert_eq!(store.issues().len(), 1);
        store.cancel(edit);
        assert_eq!(store.volume, 0.8);

        let edit = store.begin_edit();
        store.preview(edit, |values| values.volume = 0.5);
        store.commit(edit);
        assert_eq!(store.committed().volume, 0.5);
        assert!(store.persist_requested);
    }
}
