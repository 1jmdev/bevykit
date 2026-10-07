//! Headless tests of panel composition, modal behavior, bindings, and widget values.

use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::InputPlugin;
use bevy::input_focus::InputFocusPlugin;
use bevy::prelude::*;
use bevy::text::TextPlugin;
use bevy::ui::UiPlugin;
use bevy::window::WindowPlugin;
use bevykit_input::prelude::*;
use bevykit_ui::prelude::*;
use bevykit_ui::progress::ProgressBar;

#[derive(Resource, Default)]
struct JobStatus {
    progress: f32,
    ready: bool,
}

#[derive(Message, Clone)]
#[allow(dead_code)]
struct SetVolume(f32);

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        InputPlugin,
        WindowPlugin {
            primary_window: None,
            ..default()
        },
        bevy::image::ImagePlugin::default(),
        TextPlugin,
        UiPlugin,
        InputFocusPlugin,
        bevy::picking::DefaultPickingPlugins,
        KitUiPlugin::default(),
    ))
    .init_asset::<bevy::image::TextureAtlasLayout>()
    .init_resource::<JobStatus>()
    .add_message::<SetVolume>();
    app.update();
    app
}

fn open_settings(mut ui: Ui) {
    ui.panel(PanelId::new("settings"))
        .title("Settings")
        .modal()
        .build(|ui| {
            ui.progress().bind_resource(|job: &JobStatus| job.progress);
            ui.toggle("Reduce motion").value(false);
            ui.slider("Music")
                .range(0.0..=1.0)
                .value(0.5)
                .on_change(SetVolume);
            ui.button("Collect")
                .enabled_when(|job: &JobStatus| job.ready)
                .initial_focus();
        });
}

#[test]
fn modal_panel_blocks_gameplay_and_closes_on_cancel() {
    let mut app = test_app();
    app.world_mut().run_system_once(open_settings).unwrap();
    app.update();

    let panels = app.world().resource::<Panels>();
    let root = panels.entity(PanelId::new("settings")).expect("panel is open");
    assert!(
        !app.world()
            .resource::<InputContexts>()
            .is_active(&InputContext::Gameplay)
    );

    // The disabled button follows the resource through its binding.
    let mut buttons = app
        .world_mut()
        .query_filtered::<&WidgetState, With<Pressable>>();
    assert!(buttons.iter(app.world()).any(|state| state.disabled));
    app.world_mut().resource_mut::<JobStatus>().ready = true;
    app.world_mut().resource_mut::<JobStatus>().progress = 0.25;
    app.update();
    let mut buttons = app
        .world_mut()
        .query_filtered::<&WidgetState, With<Button>>();
    assert!(buttons.iter(app.world()).all(|state| !state.disabled));
    let mut bars = app.world_mut().query::<&ProgressBar>();
    assert_eq!(bars.single(app.world()).unwrap().value, 0.25);

    // Cancelling closes the dismissible modal and restores gameplay input.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    app.update();
    assert!(app.world().get_entity(root).is_err());
    assert!(
        app.world()
            .resource::<InputContexts>()
            .is_active(&InputContext::Gameplay)
    );
}

#[test]
fn toggle_flips_on_activation() {
    let mut app = test_app();
    app.world_mut().run_system_once(open_settings).unwrap();
    app.update();

    let mut toggles = app.world_mut().query::<(Entity, &Toggle)>();
    let (toggle, state) = toggles.single(app.world()).unwrap();
    assert!(!state.value);
    app.world_mut().trigger(Activated { entity: toggle });
    app.update();
    assert!(app.world().get::<Toggle>(toggle).unwrap().value);
    assert!(app.world().get::<WidgetState>(toggle).unwrap().selected);
}
