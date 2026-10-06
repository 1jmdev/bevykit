//! Integration tests for content definitions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevykit_data::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, ContentDefinition, Debug)]
#[content(kind = "item")]
struct ItemDefinition {
    id: ContentId<Self>,
    name: LocalizedKey,
    max_stack: u32,
    #[content(reference)]
    #[serde(default)]
    effects: Vec<ContentId<EffectDefinition>>,
}

#[derive(Deserialize, ContentDefinition, Debug)]
struct EffectDefinition {
    #[content(id)]
    key: ContentId<Self>,
    strength: f32,
}

fn write_content(name: &str, items: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bevykit-content-{name}-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data/items/nested")).unwrap();
    std::fs::write(root.join("data/items/potions.ron"), items).unwrap();
    std::fs::write(
        root.join("data/items/nested/sword.json"),
        r#"{ "id": "sword", "name": "items.sword", "max_stack": 1 }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("data/effects.toml"),
        "[[definitions]]\nkey = \"heal\"\nstrength = 5.0\n",
    )
    .unwrap();
    root
}

fn test_app(root: &Path) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: root.to_string_lossy().into_owned(),
            ..default()
        },
        KitContentPlugin,
    ));
    let mut registry = app.world_mut().resource_mut::<ContentRegistry>();
    registry
        .register::<ItemDefinition>()
        .load_directory("data/items")
        .validate(|item| {
            require!(item.max_stack > 0, field = "max_stack", "max_stack must be positive");
        });
    registry
        .register::<EffectDefinition>()
        .load_file("data/effects.toml");
    app
}

fn settle(app: &mut App) -> ContentStatus {
    for _ in 0..2000 {
        app.update();
        let status = app.world().resource::<ContentRegistry>().status().clone();
        if !matches!(status, ContentStatus::Loading | ContentStatus::Unloaded)
            && app.world().resource::<ContentRegistry>().revision() > 0
            || matches!(status, ContentStatus::Rejected(_))
        {
            return status;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("content never settled");
}

#[test]
fn valid_content_is_published() {
    let root = write_content(
        "valid",
        r#"[
            (id: "healing_item", name: "items.healing", max_stack: 10, effects: ["heal"]),
            (id: "elixir", name: "items.elixir", max_stack: 5),
        ]"#,
    );
    let mut app = test_app(&root);
    let status = settle(&mut app);
    assert_eq!(status, ContentStatus::Ready);

    let items = app.world().resource::<Definitions<ItemDefinition>>();
    assert_eq!(items.len(), 3);
    let healing = items.get(ContentId::<ItemDefinition>::new("healing_item")).unwrap();
    assert_eq!(healing.max_stack, 10);
    assert_eq!(healing.name.0, "items.healing");
    assert_eq!(items.get("sword").unwrap().max_stack, 1);

    let effects = app.world().resource::<Definitions<EffectDefinition>>();
    assert_eq!(effects.get("heal").unwrap().strength, 5.0);
    assert_eq!(effects.revision(), items.revision());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_content_reports_every_problem() {
    let root = write_content(
        "invalid",
        r#"[
            (id: "healing_item", name: "a", max_stack: 0, effects: ["missing_effect"]),
            (id: "sword", name: "b", max_stack: 3),
        ]"#,
    );
    let mut app = test_app(&root);
    let ContentStatus::Rejected(errors) = settle(&mut app) else {
        panic!("expected rejection");
    };
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(
        messages.iter().any(|m| m.contains("duplicate identifier")),
        "{messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains(".effects") && m.contains("missing_effect")),
        "{messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains(".max_stack") && m.contains("positive")),
        "{messages:#?}"
    );
    assert!(app.world().get_resource::<Definitions<ItemDefinition>>().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
