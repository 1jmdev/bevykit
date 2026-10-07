# bevykit_media

Audio, scene, and animation helpers for Bevy, built on `bevy_audio`, `bevy_animation`, and
`bevy_world_serialization` so the underlying components stay accessible.

- **Buses**: `Master`, `Music`, `Effects`, `Ui`, `Voice`, and game-defined buses with volume,
  mute, and pause; every bus is scaled by `Master`.
- **Cues**: named sounds with random variants, volume and pitch ranges, cooldowns, instance
  limits, and voice stealing.
- **Spatial sound**: instances that follow entities, with a policy for lost sources.
- **Music**: playlists with shuffle and repeat, crossfades and ducking on real time.
- **Pause policy**: per-bus behavior while gameplay is paused; every sink pauses in the
  background.
- **Scene bindings**: components inserted on named scene nodes once the instance is ready.
- **Animation**: clip libraries discovered from glTF scenes, clips played by name with
  blending and layers, markers, and completion events.

## Features

| Feature     | Default | Effect                                         |
| ----------- | ------- | ---------------------------------------------- |
| `audio`     | yes     | Buses, cues, spatial sound, music, `Audio`     |
| `scene`     | no      | Scene bindings                                 |
| `animation` | yes     | Clip libraries, `Animations`, markers; `scene` |

## Audio

```rust,ignore
app.add_plugins(KitAudioPlugin);

fn register(mut audio: Audio, sounds: Res<SoundAssets>) {
    audio
        .register(SoundId::Confirm)
        .source(sounds.confirm.clone())
        .bus(AudioBus::Ui)
        .max_instances(4);
    audio
        .music()
        .register(Music::Exploration)
        .track(sounds.forest.clone())
        .shuffle(true);
}

fn play(mut audio: Audio, player: Single<Entity, With<Player>>) {
    audio.play(SoundId::Confirm);
    audio.play(SoundId::Footstep).at_entity(*player).pitch(1.1);
    audio.music().transition_to(Music::Exploration).fade(1.5);
    audio.bus(AudioBus::Music).set_volume(0.8);
}
```

Settings integration, without a dependency on `bevykit_data`:

```rust,ignore
fn apply_settings(settings: Res<GameSettings>, mut mixer: ResMut<AudioMixer>) {
    if settings.is_changed() {
        mixer.bus(AudioBus::Music).set_volume(settings.music_volume);
    }
}
```

## Scenes and animation

```rust,ignore
app.add_plugins(KitAnimationPlugin);

commands.spawn((
    WorldAssetRoot(assets.load("knight.glb#Scene0")),
    SceneBindings::new().require_node("Hand.R", WeaponSocket),
));

fn animate(mut animations: Animations, knight: Single<Entity, With<Knight>>) {
    animations.on(*knight).play("Run").repeat().blend_in(0.15);
}
```

Marker times come from the game through `AnimationMarkers`.
