# bevykit

The facade crate of the bevykit toolkit for Bevy 0.19. It re-exports the member crates behind
Cargo features, provides one `prelude`, the `KitPlugins` group, and the derive macros.

## Usage

```toml
[dependencies]
bevy = "0.19"
bevykit = "0.1"
```

```rust,ignore
use bevy::prelude::*;
use bevykit::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(KitPlugins)
    .run();
```

## Re-exported modules

| Path              | Crate            | Enabled by                                                   |
| ----------------- | ---------------- | ------------------------------------------------------------ |
| `bevykit::core`   | `bevykit_core`   | always                                                       |
| `bevykit::input`  | `bevykit_input`  | `input`                                                      |
| `bevykit::ui`     | `bevykit_ui`     | `ui`                                                         |
| `bevykit::data`   | `bevykit_data`   | `assets`, `content`, `save`, `settings`, or `localization`   |

Derive macros (`KitAction`, `Settings`, `SaveData`, `AssetCollection`, `ContentDefinition`) are
exported at the crate root and resolve their paths through this crate.

## Features

| Feature        | Default | Effect                                                    |
| -------------- | ------- | --------------------------------------------------------- |
| `input`        | yes     | Actions, contexts, pointers, gestures                     |
| `ui`           | yes     | Widgets, panels, focus, bindings (implies `input`)        |
| `feedback`     | no      | Alias for `ui`; floating text, notifications, camera shake |
| `assets`       | yes     | Asset collections and loading groups                      |
| `content`      | yes     | Typed content definitions                                 |
| `save`         | yes     | Versioned save slots                                      |
| `settings`     | yes     | Persisted settings                                        |
| `localization` | yes     | Translations; also localizes UI text                      |
| `mobile`       | no      | Reserved for platform adapters                            |

## `KitPlugins`

Adds the non-generic plugins enabled by features: `KitCorePlugin`, `KitUiPlugin`,
`KitAssetsPlugin`, `KitContentPlugin`, and `KitSavePlugin`. Plugins that depend on game types
are added separately:

```rust,ignore
app.add_plugins((
    KitPlugins,
    KitInputPlugin::<GameAction>::default(),
    KitSettingsPlugin::<GameSettings>::default(),
    KitLocalizationPlugin::new("en").language("en", "locales/en.toml"),
));
```

## Example

`examples/menu.rs` builds a main menu, a settings dialog with tabs, sliders, and a toggle, and
a HUD with bound labels, a countdown, floating text, and notifications:

```sh
cargo run -p bevykit --example menu
```
