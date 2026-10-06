//! Integration tests for tweens, scoped tasks, and lifecycle handling.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::AppLifecycle;
use bevykit_core::prelude::*;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, KitCorePlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)));
    app.update();
    app
}

#[test]
fn tween_reaches_target_and_queue_waits() {
    let mut app = test_app();
    let entity = app.world_mut().spawn(Transform::default()).id();

    app.world_mut()
        .run_system_once(move |mut tweens: Tweens| {
            tweens
                .entity(entity)
                .translation(Vec3::ZERO..=Vec3::X)
                .duration(0.2)
                .ease(EaseFunction::Linear);
            tweens
                .entity(entity)
                .translation_to(Vec3::Y)
                .duration(0.2)
                .ease(EaseFunction::Linear)
                .queue();
        })
        .unwrap();

    // The queued tween must not move the entity until the first one has finished.
    let mut reached_first_target = false;
    for _ in 0..10 {
        app.update();
        let translation = app.world().get::<Transform>(entity).unwrap().translation;
        if translation.x >= 1.0 - 1e-4 {
            assert!(translation.y.abs() < 1e-4, "{translation}");
            reached_first_target = true;
            break;
        }
    }
    assert!(reached_first_target);

    for _ in 0..10 {
        app.update();
    }
    let translation = app.world().get::<Transform>(entity).unwrap().translation;
    assert!(translation.distance(Vec3::Y) < 1e-4, "{translation}");
    assert!(app.world().get::<bevykit_core::tween::ActiveTweens<Transform>>(entity).is_none());
}

#[derive(Resource, Default)]
struct Applied(u32);

#[test]
fn closing_a_scope_cancels_its_tasks() {
    let mut app = test_app();
    app.init_resource::<Applied>();

    let scope = app
        .world_mut()
        .run_system_once(|mut scopes: Scopes, mut tasks: KitTasks| {
            let open = scopes.create("open");
            let closed = scopes.create("closed");
            for scope in [open, closed] {
                tasks.spawn_in(scope, async {
                    |world: &mut World| world.resource_mut::<Applied>().0 += 1
                });
            }
            closed
        })
        .unwrap();
    app.world_mut()
        .run_system_once(move |mut scopes: Scopes| scopes.close(scope))
        .unwrap();

    for _ in 0..50 {
        app.update();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(app.world().resource::<Applied>().0, 1);
}

#[derive(Resource, Default)]
struct BackgroundCalls(u32);

#[test]
fn backgrounding_pauses_and_runs_hooks() {
    let mut app = test_app();
    app.init_resource::<BackgroundCalls>().add_systems(
        OnBackground,
        |mut calls: ResMut<BackgroundCalls>| calls.0 += 1,
    );

    app.world_mut().write_message(AppLifecycle::WillSuspend);
    app.update();
    assert_eq!(app.world().resource::<BackgroundCalls>().0, 1);
    assert!(app.world().resource::<PauseState>().is_paused());
    assert!(app.world().resource::<Time<Virtual>>().is_paused());

    app.world_mut().write_message(AppLifecycle::WillResume);
    app.update();
    assert!(!app.world().resource::<PauseState>().is_paused());
}

use bevy::ecs::system::RunSystemOnce;
