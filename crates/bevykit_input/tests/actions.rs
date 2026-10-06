//! Integration tests for action evaluation, contexts, and fixed-tick delivery.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevykit_input::prelude::*;

#[derive(KitAction, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum TestAction {
    Move,
    Interact,
    #[action(name = "open_menu")]
    Pause,
}

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(KitInputPlugin::<TestAction>::default())
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
    app.world_mut()
        .resource_mut::<InputMap<TestAction>>()
        .context(InputContext::Gameplay)
        .axis2(TestAction::Move, KeyboardAxis::wasd())
        .button(TestAction::Interact, KeyCode::KeyE);
    app.world_mut()
        .resource_mut::<InputMap<TestAction>>()
        .context(InputContext::PauseMenu)
        .button(TestAction::Pause, KeyCode::Escape);
    app.update();
    app
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
}

fn release(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(key);
}

fn clear_edges(app: &mut App) {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
}

#[test]
fn derive_names_variants() {
    assert_eq!(TestAction::Move.name(), "move");
    assert_eq!(TestAction::Pause.name(), "open_menu");
    assert_eq!(TestAction::from_name("interact"), Some(TestAction::Interact));
    assert_eq!(TestAction::variants().len(), 3);
}

#[test]
fn button_edges_and_axis() {
    let mut app = test_app();

    press(&mut app, KeyCode::KeyE);
    press(&mut app, KeyCode::KeyW);
    press(&mut app, KeyCode::KeyD);
    app.update();
    clear_edges(&mut app);

    let state = app.world().resource::<ActionState<TestAction>>();
    assert!(state.just_pressed(TestAction::Interact));
    let axis = state.axis2(TestAction::Move);
    assert!((axis.length() - 1.0).abs() < 1e-5);
    assert!(axis.x > 0.0 && axis.y > 0.0);

    app.update();
    let state = app.world().resource::<ActionState<TestAction>>();
    assert!(state.pressed(TestAction::Interact));
    assert!(!state.just_pressed(TestAction::Interact));

    release(&mut app, KeyCode::KeyE);
    app.update();
    let state = app.world().resource::<ActionState<TestAction>>();
    assert!(state.just_released(TestAction::Interact));
}

#[test]
fn blocked_context_reads_released() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<InputContexts>()
        .push(InputContext::PauseMenu)
        .block(InputContext::Gameplay);

    press(&mut app, KeyCode::KeyE);
    press(&mut app, KeyCode::Escape);
    app.update();

    let state = app.world().resource::<ActionState<TestAction>>();
    assert!(!state.pressed(TestAction::Interact));
    assert!(state.pressed(TestAction::Pause));
}

#[test]
fn fixed_state_delivers_each_press_once() {
    let mut fixed = App::new();
    fixed
        .add_plugins(MinimalPlugins)
        .add_plugins(KitInputPlugin::<TestAction>::default())
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<PressCount>()
        .insert_resource(Time::<Fixed>::from_seconds(0.05))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(20)))
        .add_systems(FixedUpdate, count_presses);
    fixed
        .world_mut()
        .resource_mut::<InputMap<TestAction>>()
        .context(InputContext::Gameplay)
        .button(TestAction::Interact, KeyCode::KeyE);

    // Frames of 20 ms against ticks of 50 ms: most frames run no tick.
    press(&mut fixed, KeyCode::KeyE);
    for _ in 0..10 {
        fixed.update();
        clear_edges(&mut fixed);
    }
    assert_eq!(fixed.world().resource::<PressCount>().0, 1);
}

#[derive(Resource, Default)]
struct PressCount(u32);

fn count_presses(actions: Res<FixedActionState<TestAction>>, mut count: ResMut<PressCount>) {
    if actions.just_pressed(TestAction::Interact) {
        count.0 += 1;
    }
}

#[test]
fn saved_bindings_round_trip_and_conflicts() {
    let mut map = InputMap::<TestAction>::default();
    map.context(InputContext::Gameplay)
        .button(TestAction::Interact, KeyCode::KeyE)
        .button(TestAction::Pause, KeyCode::Escape);

    map.rebind(&InputContext::Gameplay, TestAction::Pause, 0, ButtonBinding::Key(KeyCode::KeyE));
    let conflicts = map.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].actions, vec![TestAction::Interact, TestAction::Pause]);

    let saved = serde_json::to_string(&map.save()).unwrap();
    map.reset_to_defaults();
    assert!(map.conflicts().is_empty());

    let unknown = map.load(&serde_json::from_str(&saved).unwrap());
    assert!(unknown.is_empty());
    assert_eq!(map.conflicts().len(), 1);
    assert_eq!(map.describe(TestAction::Pause), vec!["E".to_string()]);
}
