# bevykit

**A modular Rust toolkit that makes the common parts of game development work together on top
of Bevy 0.19.**

The game supplies its content and writes its gameplay systems. bevykit handles the repeated
infrastructure around them: a dialog captures input, a button cancels when a press becomes a
scroll, a label follows a resource, a setting survives restarting, and a save survives a crash.

bevykit ships code and behavior only. Fonts, images, sounds, and every other asset come from the
game. It keeps Bevy's programming model: components, resources, systems, and plugins, with the
underlying Bevy data always accessible.

## Crates

| Crate            | Contents                                                                   |
| ---------------- | -------------------------------------------------------------------------- |
| `bevykit`        | Facade: re-exports, `prelude`, `KitPlugins`, Cargo features                |
| `bevykit_core`   | Keys, scopes, scoped tasks, pause, deadlines, tweens, lifecycle, haptics   |
| `bevykit_input`  | Actions, contexts, rebinding, fixed-tick input, pointers, gestures         |
| `bevykit_ui`     | Themes, widgets, focus, panels, bindings, world anchors, feedback          |
| `bevykit_data`   | Asset groups, content definitions, saves, settings, localization           |
| `bevykit_media`  | Audio buses, cues, music, scene bindings, animation clips and markers      |
| `bevykit_macros` | Derive macros                                                              |

Audio, scene, and animation helpers live in `bevykit_media`.

## Setup

```rust,ignore
use bevy::prelude::*;
use bevykit::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            KitPlugins,
            KitInputPlugin::<GameAction>::default(),
            KitSettingsPlugin::<GameSettings>::default(),
        ))
        .run();
}
```

## Input actions and contexts

Game code reads named actions instead of keys, fingers, or buttons. Every action has a button
state, a scalar value, and a 2D axis.

```rust,ignore
#[derive(KitAction, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum GameAction {
    Move,
    Interact,
    Pause,
}

fn configure_input(mut input: ResMut<InputMap<GameAction>>) {
    input
        .context(InputContext::Gameplay)
        .axis2(GameAction::Move, KeyboardAxis::wasd())
        .axis2(GameAction::Move, GamepadStick::Left)
        .button(GameAction::Interact, KeyCode::KeyE)
        .button(GameAction::Interact, GamepadButton::South)
        .button(GameAction::Pause, KeyCode::Escape);
}

fn open_pause_menu(mut contexts: ResMut<InputContexts>) {
    contexts.push(InputContext::PauseMenu).block(InputContext::Gameplay);
}

fn interact(actions: Res<ActionState<GameAction>>) {
    if actions.just_pressed(GameAction::Interact) {
        // The game's interaction rules.
    }
}
```

- `FixedActionState<A>` delivers each press to exactly one `FixedUpdate` tick.
- `Rebinding<A>` captures the next input, reports conflicts, and `InputMap::save` persists
  bindings with serde.
- Input clears when the window loses focus or the app is suspended.

## Pointers and gestures

Mouse and touch share one pointer model with single-owner capture.

```rust,ignore
fn begin_drag(mut pointers: ResMut<PointerRouter>, targets: Query<Entity, With<Draggable>>) {
    for entity in &targets {
        if let Some(press) = pointers.press_on(entity) {
            pointers.capture(press.id, entity);
        }
    }
}

commands
    .spawn((
        InteractionSurface::screen(),
        GesturePolicy::default().tap().drag_after_distance(8.0).pinch(),
    ))
    .observe(|pinch: On<GesturePinch>| info!("zoom by {}", pinch.scale));
```

Taps, double taps, holds, drags, pinches, and rotations are recognized in logical pixels.
`VirtualButton` and `VirtualStick` turn game-styled UI nodes into controls that drive the same
actions.

## UI

Screens are composed once when opened and then update through bindings, so focus, scroll
positions, and in-progress presses survive.

```rust,ignore
fn open_settings(mut ui: Ui, settings: Res<SettingsStore<GameSettings>>) {
    ui.panel(PanelId::new("settings"))
        .title("Settings")
        .modal()
        .preserve_scroll()
        .build(|ui| {
            ui.slider("Music")
                .range(0.0..=1.0)
                .value(settings.music_volume)
                .on_change(SetMusicVolume);

            ui.toggle("Reduce motion").value(settings.reduce_motion);

            ui.button("Close").primary().initial_focus().send(CloseSettings);
        });
}
```

- Widgets: buttons, labels, toggles, sliders, progress bars, countdowns, text fields, tabs,
  tooltips, scroll views, virtualized lists, images, rows, and columns.
- Buttons activate on release and cancel when a scroll view takes the pointer.
- Keyboard and controller navigation is spatial, with `FocusLinks` overrides. Modal panels
  trap focus, block gameplay input, and restore focus when closed.
- Themes define colors, typography, spacing, and shape:

```rust,ignore
ui.set_theme(
    UiTheme::new()
        .font(game_assets.ui_font.clone())
        .spacing(8.0)
        .text_color(Color::WHITE)
        .accent_color(Color::srgb(0.25, 0.65, 0.9)),
);
```

### Bindings and world anchors

```rust,ignore
ui.progress().bind_resource(|job: &JobStatus| job.progress());
ui.countdown().bind_resource(|job: &JobStatus| Some(job.deadline));
ui.button("Collect")
    .enabled_when(|job: &JobStatus| job.ready)
    .send(CollectJob);

ui.panel(PanelId::new("object-details"))
    .anchor(WorldAnchor::entity(target).camera(main_camera))
    .screen_offset(Vec2::new(0.0, -24.0))
    .clamp_to_safe_area()
    .build(build_object_details);
```

### Feedback

```rust,ignore
fn reward(mut feedback: Feedback, target: Single<Entity, With<Chest>>, camera: Single<Entity, With<Camera>>) {
    feedback.floating_text("+10").at_entity(*target).style(FeedbackStyle::Reward).duration(0.8);
    feedback.notify("Chest opened");
    feedback.shake(*camera, 0.4);
    feedback.haptic(HapticPattern::Success);
}
```

Feedback cleans itself up and respects `FeedbackSettings` for reduced motion and haptics.

## Assets and content

```rust,ignore
#[derive(Resource, AssetCollection)]
struct GameAssets {
    #[asset(path = "fonts/interface.ttf")]
    ui_font: Handle<Font>,
    #[asset(path = "world/level.glb#Scene0")]
    level_scene: Handle<WorldAsset>,
}

fn configure_assets(mut assets: ResMut<AssetGroups>) {
    assets
        .group(AssetGroup::Level)
        .depends_on(AssetGroup::Shared)
        .collection::<GameAssets>()
        .required("world/level.glb")
        .optional("audio/ambience.ogg", MissingAssetPolicy::Skip)
        .initialize(bind_level_content);
    assets.load(AssetGroup::Level);
}
```

Groups report progress, failures with the file and requester, retry, and unload. Readiness
includes dependencies and initialization steps.

```rust,ignore
#[derive(Deserialize, ContentDefinition)]
struct ItemDefinition {
    id: ContentId<Self>,
    max_stack: u32,
    #[content(reference)]
    upgrades_to: Option<ContentId<ItemDefinition>>,
}

fn configure_content(mut content: ResMut<ContentRegistry>) {
    content
        .register::<ItemDefinition>()
        .load_directory("data/items")
        .validate(|item| {
            require!(item.max_stack > 0, "max_stack must be positive");
        });
}

fn inspect(items: Res<Definitions<ItemDefinition>>) {
    let item = items.get("healing_item");
}
```

Definitions load from RON, JSON, or TOML. Duplicates, missing references, and failed
validators are reported with file, ID, and field; only a fully valid revision is published.

## Saves and settings

```rust,ignore
#[derive(Serialize, Deserialize, SaveData)]
#[save(version = 3)]
struct GameSave {
    player: PlayerSave,
    completed_levels: Vec<String>,
}

app.add_plugins(KitSavePlugin::default());
app.register_save::<GameSave>()
    .snapshot(capture_game)
    .restore(restore_game)
    .migrate(2, migrate_v2_to_v3);

fn checkpoint(mut saves: ResMut<Saves>) {
    saves.request(SaveSlot::Auto);
}
```

Writes are asynchronous and ordered, files are replaced atomically with a backup, corrupt files
recover from the backup, and failed loads leave the session untouched. `SaveId` keeps references
stable across save and load.

```rust,ignore
#[derive(Serialize, Deserialize, Settings, Clone, Default)]
struct GameSettings {
    #[setting(range = 0.0..=1.0)]
    music_volume: f32,
    reduce_motion: bool,
}

let edit = settings.begin_edit();
settings.preview(edit, |values| values.music_volume = 0.5);
settings.commit(edit);
```

## Localization

Translation files are TOML with nested keys, plural forms, and interpolation:

```rust,ignore
ui.label(tr!("inventory.count", count = inventory.len()));
let text = locale.translate(&tr!("welcome", player = name));
```

## Scopes, tasks, pause, and time

```rust,ignore
let scope = scopes.create("level-session");
commands.spawn((LevelObject, OwnedBy(scope)));
tasks.spawn_in(scope, load_level_metadata());
scopes.close(scope);

app.add_systems(FixedUpdate, simulate_game.run_if(gameplay_running));

let deadline = deadlines.after(Duration::from_secs(300));
tweens.entity(panel).scale(Vec3::splat(0.9)..=Vec3::ONE).duration(0.2).ease(EaseFunction::CubicOut);
```

Closing a scope despawns what it owns and cancels its tasks. Pause is reference-counted by
reason and stops virtual time while presentation keeps running. Deadlines use wall-clock time
and survive restarts. `OnBackground` and `OnForeground` schedules run on mobile suspension.

## Schedule

| Schedule     | Set                        | Purpose                                  |
| ------------ | -------------------------- | ---------------------------------------- |
| `PreUpdate`  | `KitSystems::Platform`     | Lifecycle, display, and clock sampling   |
| `PreUpdate`  | `KitSystems::Input`        | Action state, pointers, and gestures     |
| `PreUpdate`  | `KitSystems::Interaction`  | Focus, activation, and widget behavior   |
| `Update`     | game systems               | Gameplay reads the input produced above  |
| `PostUpdate` | `KitSystems::Bindings`     | Data bindings and text, before layout    |
| `PostUpdate` | `KitSystems::Presentation` | Tweens, anchors, and feedback            |
| `Last`       | `KitSystems::Cleanup`      | Task completion, expiry, and saves       |

## License

MIT or Apache-2.0, at your option.
