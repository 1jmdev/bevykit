//! Integration tests for asset collections and loading groups.

use std::path::PathBuf;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevykit_data::localization::{TranslationFile, TranslationLoader};
use bevykit_data::prelude::*;

#[derive(Resource, AssetCollection)]
struct TextAssets {
    #[asset(path = "english.toml")]
    english: Handle<TranslationFile>,
    #[asset(paths("english.toml", "czech.toml"))]
    all: Vec<Handle<TranslationFile>>,
    unrelated: u32,
}

#[derive(Resource, Default)]
struct Initialized(u32);

fn asset_directory(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bevykit-assets-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("english.toml"), "hello = \"Hello\"").unwrap();
    std::fs::write(root.join("czech.toml"), "hello = \"Ahoj\"").unwrap();
    root
}

fn test_app(root: &std::path::Path) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.to_string_lossy().into_owned(),
            ..default()
        },
        KitAssetsPlugin,
    ))
    .init_asset::<TranslationFile>()
    .init_asset_loader::<TranslationLoader>()
    .init_resource::<Initialized>();
    app
}

fn wait_for(app: &mut App, group: &AssetGroup, done: impl Fn(&GroupStatus) -> bool) -> GroupStatus {
    for _ in 0..2000 {
        app.update();
        let status = app.world().resource::<AssetGroups>().status(group);
        if done(&status) {
            return status;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("group never settled");
}

#[test]
fn group_becomes_ready_after_assets_and_initialization() {
    let root = asset_directory("ready");
    let mut app = test_app(&root);
    {
        let mut groups = app.world_mut().resource_mut::<AssetGroups>();
        groups
            .group(AssetGroup::Shared)
            .required_typed::<TranslationFile>("czech.toml");
        groups
            .group(AssetGroup::Level)
            .depends_on(AssetGroup::Shared)
            .collection::<TextAssets>()
            .optional("missing.toml", MissingAssetPolicy::Skip)
            .initialize(|mut initialized: ResMut<Initialized>| initialized.0 += 1);
        groups.load(AssetGroup::Level);
    }

    let status = wait_for(&mut app, &AssetGroup::Level, |status| {
        matches!(status, GroupStatus::Ready | GroupStatus::Failed(_))
    });
    assert!(matches!(status, GroupStatus::Ready), "{status:?}");
    assert!(app.world().resource::<AssetGroups>().is_ready(&AssetGroup::Shared));
    assert_eq!(app.world().resource::<Initialized>().0, 1);

    let collection = app.world().resource::<TextAssets>();
    assert_eq!(collection.all.len(), 2);
    assert_eq!(collection.unrelated, 0);
    let files = app.world().resource::<Assets<TranslationFile>>();
    assert_eq!(files.get(&collection.english).unwrap().len(), 1);

    let czech = app
        .world()
        .resource::<AssetGroups>()
        .typed_handle::<TranslationFile>(&AssetGroup::Shared, "czech.toml");
    assert!(czech.is_some());

    app.world_mut()
        .resource_mut::<AssetGroups>()
        .unload(AssetGroup::Level);
    app.update();
    assert!(app.world().get_resource::<TextAssets>().is_none());
    assert!(matches!(
        app.world().resource::<AssetGroups>().status(&AssetGroup::Level),
        GroupStatus::Unloaded
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_required_asset_fails_with_its_path() {
    let root = asset_directory("failure");
    let mut app = test_app(&root);
    {
        let mut groups = app.world_mut().resource_mut::<AssetGroups>();
        groups
            .group(AssetGroup::Menu)
            .required_typed::<TranslationFile>("absent.toml")
            .initialize(|mut initialized: ResMut<Initialized>| initialized.0 += 1);
        groups.load(AssetGroup::Menu);
    }

    let status = wait_for(&mut app, &AssetGroup::Menu, |status| {
        matches!(status, GroupStatus::Ready | GroupStatus::Failed(_))
    });
    let GroupStatus::Failed(failures) = status else {
        panic!("expected failure, got {status:?}");
    };
    assert_eq!(failures[0].path, "absent.toml");
    assert_eq!(app.world().resource::<Initialized>().0, 0);
    std::fs::remove_dir_all(root).unwrap();
}
