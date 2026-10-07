# bevykit_media

Planned crate, not yet implemented. It completes sections 9 (audio) and 10 (scene and
animation helpers) of the design. It follows the same conventions as the other crates: flat
modules in `src/`, one concern per file, plugins that call `bevykit_core::ensure_core`, and a
`prelude` re-exported by the `bevykit` facade behind the `audio` and `animation` features.

## Dependencies

- `bevy` with `bevy_audio`, `bevy_animation`, `bevy_scene`, `bevy_world_serialization`.
- `bevykit_core` for `Key`, `PauseState`, `LifecycleState`, `KitSystems`, and `TimeDomain`.
- `fastrand` for variant selection and pitch/volume randomization.

Built on `bevy_audio` (`AudioPlayer`, `PlaybackSettings`, `AudioSink`, `SpatialAudioSink`,
`GlobalVolume`, `Volume`) so the backend stays accessible to games.

## Audio

### `bus.rs`: volume buses
- `AudioBus` identifiers: `Master`, `Music`, `Effects`, `Ui`, `Voice`, plus `AudioBus::new(name)`.
- `AudioMixer` resource: per-bus volume, mute, and pause; effective volume is the product of
  the bus and its parent (`Master`).
- `audio.bus(AudioBus::Music).set_volume(0.8)`, `.pause()`, `.resume()`, `.mute(bool)`.
- A system pushes effective volumes to every playing sink whenever the mixer changes.

### `cue.rs`: named cues
- `CueId` built with `define_key!`, so games can use strings or their own `SoundId` enum.
- `Cue`: sources (variants chosen at random, never the same twice in a row), bus, volume and
  pitch ranges, cooldown, `max_instances`, and a `VoiceStealing` policy (`Oldest`, `Quietest`,
  `Reject`).
- `audio.register(SoundId::Confirm).source(handle).bus(AudioBus::Ui).max_instances(4)`.

### `playback.rs`: the `Audio` system parameter
- `audio.play(cue)` returns a builder: `.volume()`, `.pitch()`, `.at_entity(e)`,
  `.at_point(v)`, `.looping()`, `.in_scope(scope)` (stops when the scope closes).
- Instances are entities with `PlayingCue { cue, bus, base_volume }`, owned by `OwnedBy` when
  scoped.

### `spatial.rs`: positional sound
- Instances following an entity copy its `GlobalTransform` each frame.
- `SourceLost` policy for when the followed entity despawns: `Stop`, `FadeOut(secs)`,
  `KeepPosition`.
- A listener helper that attaches `SpatialListener` to the chosen camera.

### `music.rs`: music and transitions
- `MusicId` and a playlist registry: tracks, shuffle, repeat.
- `audio.music().transition_to(MusicId::Exploration).fade(1.5)`: crossfade using two sinks,
  driven by real time so fades continue while gameplay is paused.
- `.stop().fade(secs)` and `.duck(amount, secs)` for dialogue.

### `policy.rs`: pause and lifecycle
- `AudioPausePolicy` per bus: pause with gameplay (`PauseState`), keep playing (UI, music), or
  duck.
- Background: pause every sink in `OnBackground`, resume in `OnForeground`.

### Settings integration (documented pattern, no dependency on `bevykit_data`)
```rust
if settings.is_changed() {
    mixer.bus(AudioBus::Music).set_volume(settings.music_volume);
}
```

## Scenes

### `scene_bindings.rs`
- `SceneBindings` component placed next to `WorldAssetRoot`:
  `.require_node("InteractionPoint", InteractionTarget)` inserts a component on the named
  descendant, `.optional_node(name, bundle)`, `.on_ready(system)`.
- Resolution runs once on `WorldInstanceReady`: descendants are searched by `Name` a single
  time and cached in `SceneNodes { name -> Entity }` on the root.
- Missing or ambiguous names produce a `SceneBindingError { root, name, kind }` message and an
  error log; required failures skip `on_ready`.
- The cache lives on the root entity, so it is dropped with the scene instance.

## Animation

### `clips.rs`
- `AnimationLibrary` discovered from the scene: named clips from glTF, the
  `AnimationPlayer` entity, and a generated `AnimationGraph` with one node per clip.
- Built once per scene instance after `WorldInstanceReady`, stored on the scene root.

### `controller.rs`: the `Animations` system parameter
- `animations.on(character).play("Run").repeat().blend_in(0.15)`.
- `.once()`, `.speed(f32)`, `.seek(secs)`, `.layer(n)` for additive or upper-body layers.
- Uses `AnimationTransitions` for blending; exposes `AnimationPlayback { clip, elapsed,
  finished }` for queries.

### `markers.rs`
- `.notify_at("contact", InteractionContact { character })`: named markers mapped to clip
  times (from a game-supplied table or glTF extras), triggered as entity events once per pass,
  including when a frame skips over the marker time.
- `AnimationFinished { entity, clip }` event for `.once()` clips.

## Tests to write later
- Mixer volume propagation, cooldown and instance limits, variant selection.
- Crossfade volumes over time, pausing with `PauseState`.
- Scene binding resolution and error reporting with a hand-built `WorldAsset`.
- Marker delivery across large frame deltas.
