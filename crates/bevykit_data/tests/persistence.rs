//! Integration tests for saves and settings.

use std::time::Duration;

use bevy::prelude::*;
use bevykit_data::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, SaveData, Clone, Debug, PartialEq)]
#[save(version = 2)]
struct GameSave {
    level: u32,
    player_name: String,
}

#[derive(Resource, Default, Clone, Debug, PartialEq)]
struct Progress {
    level: u32,
    player_name: String,
}

#[derive(Serialize, Deserialize, Settings, Clone, Debug, PartialEq)]
#[settings(key = "test-settings.json")]
struct TestSettings {
    #[setting(range = 0.0..=1.0)]
    music_volume: f32,
    reduce_motion: bool,
}

impl Default for TestSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.7,
            reduce_motion: false,
        }
    }
}

fn capture(progress: Res<Progress>) -> GameSave {
    GameSave {
        level: progress.level,
        player_name: progress.player_name.clone(),
    }
}

fn restore(save: In<GameSave>, mut progress: ResMut<Progress>) {
    progress.level = save.level;
    progress.player_name = save.player_name.clone();
}

fn migrate_v1_to_v2(mut value: serde_json::Value) -> Result<serde_json::Value, String> {
    let object = value.as_object_mut().ok_or("expected an object")?;
    let name = object.remove("name").ok_or("missing `name`")?;
    object.insert("player_name".to_string(), name);
    Ok(value)
}

fn test_app(storage: KitStorage) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(storage)
        .init_resource::<Progress>()
        .add_plugins(KitSavePlugin::default());
    app.register_save::<GameSave>()
        .snapshot(capture)
        .restore(restore)
        .migrate(1, migrate_v1_to_v2);
    app
}

fn run_until<T: Message + Clone>(app: &mut App) -> T {
    let mut cursor = app.world().resource::<Messages<T>>().get_cursor_current();
    for _ in 0..500 {
        app.update();
        let messages = app.world().resource::<Messages<T>>();
        if let Some(message) = cursor.read(messages).next() {
            return message.clone();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("message never arrived");
}

#[test]
fn save_then_load_restores_progress() {
    let storage = KitStorage::new(MemoryStorage::default());
    let mut app = test_app(storage.clone());
    *app.world_mut().resource_mut::<Progress>() = Progress {
        level: 7,
        player_name: "Ada".to_string(),
    };

    app.world_mut().resource_mut::<Saves>().request(SaveSlot::named("one"));
    let saved: SaveCompleted = run_until(&mut app);
    assert!(saved.result.is_ok());

    *app.world_mut().resource_mut::<Progress>() = Progress::default();
    app.world_mut().resource_mut::<Saves>().load(SaveSlot::named("one"));
    let loaded: LoadCompleted = run_until(&mut app);
    assert!(loaded.result.is_ok());
    assert!(!loaded.recovered);
    assert_eq!(app.world().resource::<Progress>().level, 7);

    let listed = Saves::list(&storage).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].slot, SaveSlot::named("one"));
}

#[test]
fn old_versions_are_migrated() {
    let storage = KitStorage::new(MemoryStorage::default());
    let legacy = br#"{
        "format": 1,
        "saved_at": 0,
        "entries": { "game_save": { "version": 1, "data": { "level": 3, "name": "Old" } } }
    }"#;
    storage.write_blocking("save-auto.json", legacy).unwrap();

    let mut app = test_app(storage);
    app.world_mut().resource_mut::<Saves>().load(SaveSlot::Auto);
    let loaded: LoadCompleted = run_until(&mut app);
    assert!(loaded.result.is_ok(), "{:?}", loaded.result);
    assert_eq!(app.world().resource::<Progress>().player_name, "Old");
}

#[test]
fn corrupt_primary_recovers_from_backup_and_failure_keeps_session() {
    let storage = KitStorage::new(MemoryStorage::default());
    let mut app = test_app(storage.clone());
    app.world_mut().resource_mut::<Progress>().level = 4;
    app.world_mut().resource_mut::<Saves>().request(SaveSlot::Quick);
    let _: SaveCompleted = run_until(&mut app);

    storage.write_blocking("save-quick.json", b"{ not json").unwrap();
    app.world_mut().resource_mut::<Progress>().level = 9;
    app.world_mut().resource_mut::<Saves>().load(SaveSlot::Quick);
    let loaded: LoadCompleted = run_until(&mut app);
    assert!(loaded.recovered, "{:?}", loaded);
    assert_eq!(app.world().resource::<Progress>().level, 4);

    app.world_mut().resource_mut::<Progress>().level = 11;
    app.world_mut().resource_mut::<Saves>().load(SaveSlot::named("missing"));
    let loaded: LoadCompleted = run_until(&mut app);
    assert!(loaded.result.is_err());
    assert_eq!(app.world().resource::<Progress>().level, 11);
}

#[test]
fn settings_load_sanitize_and_persist() {
    let storage = KitStorage::new(MemoryStorage::default());
    storage
        .write_blocking("test-settings.json", br#"{ "music_volume": 4.0 }"#)
        .unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(storage.clone())
        .add_plugins(KitSettingsPlugin::<TestSettings>::default());

    let settings = app.world().resource::<SettingsStore<TestSettings>>();
    assert_eq!(settings.music_volume, 1.0);
    assert_eq!(settings.issues().len(), 1);

    app.world_mut()
        .resource_mut::<SettingsStore<TestSettings>>()
        .set(|values| values.reduce_motion = true);
    for _ in 0..200 {
        app.update();
        if *app.world().resource::<SettingsStore<TestSettings>>().status() == PersistStatus::Saved {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let stored = String::from_utf8(storage.read("test-settings.json").unwrap().unwrap()).unwrap();
    assert!(stored.contains("\"reduce_motion\": true"));
}
