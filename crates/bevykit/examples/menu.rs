//! A main menu, a settings dialog, and a small HUD built with bevykit.
//!
//! Controls: Space collects coins, Tab or M opens the menu, arrow keys and Enter (or a
//! gamepad) navigate menus, Escape closes dialogs.
//!
//! Run with `cargo run -p bevykit --example menu`.

use std::time::Duration;

use bevy::prelude::*;
use bevykit::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(KitAction, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum GameAction {
    Collect,
    OpenMenu,
}

#[derive(Serialize, Deserialize, Settings, Clone, Debug, PartialEq)]
#[settings(key = "example-settings.json")]
struct GameSettings {
    #[setting(range = 0.0..=1.0)]
    music_volume: f32,
    #[setting(range = 0.0..=1.0)]
    effects_volume: f32,
    reduce_motion: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.7,
            effects_volume: 0.8,
            reduce_motion: false,
        }
    }
}

#[derive(Resource)]
struct Score {
    points: u32,
    coins: u32,
    bonus: Deadline,
}

#[derive(Message, Clone)]
struct OpenMenu;

#[derive(Message, Clone)]
struct OpenSettings;

#[derive(Message, Clone)]
struct StartGame;

#[derive(Message, Clone)]
struct SetMusicVolume(f32);

#[derive(Message, Clone)]
struct SetEffectsVolume(f32);

#[derive(Message, Clone)]
struct SetReduceMotion(bool);

const MENU: PanelId = PanelId::from_static("menu");
const SETTINGS: PanelId = PanelId::from_static("settings");

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            KitPlugins,
            KitInputPlugin::<GameAction>::default(),
            KitSettingsPlugin::<GameSettings>::default(),
        ))
        .add_message::<OpenMenu>()
        .add_message::<OpenSettings>()
        .add_message::<StartGame>()
        .add_message::<SetMusicVolume>()
        .add_message::<SetEffectsVolume>()
        .add_message::<SetReduceMotion>()
        .add_systems(Startup, (setup, configure_input, spawn_hud, open_menu))
        .add_systems(
            Update,
            (
                open_menu.run_if(on_message::<OpenMenu>),
                open_settings.run_if(on_message::<OpenSettings>),
                start_game.run_if(on_message::<StartGame>),
                change_settings,
                apply_settings,
                collect_coins,
                request_menu,
                renew_bonus,
            ),
        )
        .run();
}

fn setup(mut commands: Commands, deadlines: Res<Deadlines>) {
    commands.spawn(Camera2d);
    commands.insert_resource(Score {
        points: 0,
        coins: 0,
        bonus: deadlines.after(Duration::from_secs(30)),
    });
}

fn configure_input(mut input: ResMut<InputMap<GameAction>>) {
    input
        .context(InputContext::Gameplay)
        .button(GameAction::Collect, KeyCode::Space)
        .button(GameAction::Collect, GamepadButton::South)
        .button(GameAction::OpenMenu, KeyCode::KeyM)
        .button(GameAction::OpenMenu, KeyCode::Tab)
        .button(GameAction::OpenMenu, GamepadButton::Start);
}

fn spawn_hud(mut ui: Ui) {
    ui.root().respect_safe_area().build(|ui| {
        ui.row(|ui| {
            ui.label("")
                .bind_resource(|score: &Score| format!("Score: {}", score.points));
            ui.spacer();
            ui.label("")
                .bind_resource(|score: &Score| format!("Coins: {}", score.coins));
        });
        ui.row(|ui| {
            ui.label("Next bonus in");
            ui.countdown()
                .bind_resource(|score: &Score| Some(score.bonus));
        });
    });
}

fn open_menu(mut ui: Ui) {
    ui.panel(MENU)
        .title("Main Menu")
        .modal()
        .width(Val::Px(360.0))
        .build(|ui| {
            ui.button("Play")
                .primary()
                .initial_focus()
                .send(StartGame);
            ui.button("Settings")
                .tooltip("Audio and display options")
                .send(OpenSettings);
            ui.button("Quit")
                .on_activate(|commands, _| {
                    commands.write_message(AppExit::Success);
                });
        });
}

fn open_settings(mut ui: Ui, settings: Res<SettingsStore<GameSettings>>) {
    ui.panel(SETTINGS)
        .title("Settings")
        .modal()
        .preserve_scroll()
        .width(Val::Px(420.0))
        .build(|ui| {
            ui.tabs(|tabs| {
                tabs.tab("Audio", |ui| {
                    ui.slider("Music volume")
                        .range(0.0..=1.0)
                        .step(0.05)
                        .value(settings.music_volume)
                        .show_value(|value| format!("{:.0}%", value * 100.0))
                        .initial_focus()
                        .on_change(SetMusicVolume);
                    ui.slider("Effects volume")
                        .range(0.0..=1.0)
                        .step(0.05)
                        .value(settings.effects_volume)
                        .show_value(|value| format!("{:.0}%", value * 100.0))
                        .on_change(SetEffectsVolume);
                });
                tabs.tab("Display", |ui| {
                    ui.toggle("Reduce motion")
                        .value(settings.reduce_motion)
                        .on_change(SetReduceMotion);
                });
            });
            ui.separator();
            ui.button("Close")
                .on_activate(|commands, _| {
                    commands.run_system_cached(close_settings);
                });
        });
}

fn close_settings(mut ui: Ui, mut feedback: Feedback) {
    ui.close(SETTINGS);
    feedback.notify("Settings saved");
}

fn start_game(mut ui: Ui) {
    ui.close(MENU);
}

fn request_menu(
    actions: Res<ActionState<GameAction>>,
    panels: Res<Panels>,
    mut open: MessageWriter<OpenMenu>,
) {
    if actions.just_pressed(GameAction::OpenMenu) && panels.entity(MENU).is_none() {
        open.write(OpenMenu);
    }
}

fn change_settings(
    mut settings: ResMut<SettingsStore<GameSettings>>,
    mut music: MessageReader<SetMusicVolume>,
    mut effects: MessageReader<SetEffectsVolume>,
    mut motion: MessageReader<SetReduceMotion>,
) {
    for SetMusicVolume(volume) in music.read() {
        settings.set(|values| values.music_volume = *volume);
    }
    for SetEffectsVolume(volume) in effects.read() {
        settings.set(|values| values.effects_volume = *volume);
    }
    for SetReduceMotion(reduce) in motion.read() {
        settings.set(|values| values.reduce_motion = *reduce);
    }
}

fn apply_settings(
    settings: Res<SettingsStore<GameSettings>>,
    mut feedback_settings: ResMut<FeedbackSettings>,
) {
    if settings.is_changed() {
        feedback_settings.reduce_motion = settings.reduce_motion;
    }
}

fn collect_coins(
    actions: Res<ActionState<GameAction>>,
    mut score: ResMut<Score>,
    mut feedback: Feedback,
    windows: Query<&Window>,
    camera: Single<Entity, With<Camera>>,
) {
    if !actions.just_pressed(GameAction::Collect) {
        return;
    }
    score.points += 10;
    score.coins += 1;
    let center = windows
        .iter()
        .next()
        .map(|window| Vec2::new(window.width(), window.height()) * 0.5)
        .unwrap_or_default();
    feedback
        .floating_text("+10")
        .at_screen(center)
        .style(FeedbackStyle::Reward)
        .duration(0.8);
    feedback.shake(*camera, 0.3);
    feedback.haptic(HapticPattern::Light);
}

fn renew_bonus(mut score: ResMut<Score>, deadlines: Res<Deadlines>, mut feedback: Feedback) {
    if deadlines.is_expired(&score.bonus) {
        score.bonus = deadlines.after(Duration::from_secs(30));
        score.points += 100;
        feedback.notify("Bonus collected!");
    }
}
