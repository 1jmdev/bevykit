//! The [`Audio`] system parameter and sound instances.
//!
//! Each playing sound is an entity with an [`AudioPlayer`], so the Bevy backend stays
//! accessible. Cue instances carry [`PlayingCue`]; scoped instances are owned by their scope
//! through [`OwnedBy`] and stop when it closes.
//!
//! ```ignore
//! fn on_hit(mut audio: Audio, hits: Query<Entity, Added<Hit>>, level: Res<LevelScope>) {
//!     for target in &hits {
//!         audio.play(SoundId::Impact).at_entity(target).pitch(1.1).in_scope(level.0);
//!     }
//! }
//! ```

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, GlobalVolume, PlaybackMode, PlaybackSettings,
    SpatialAudioSink, SpatialListener, Volume,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevykit_core::key::IntoKey;
use bevykit_core::pause::PauseState;
use bevykit_core::platform::LifecycleState;
use bevykit_core::scope::OwnedBy;

use crate::bus::{AudioBus, AudioMixer, BusState};
use crate::cue::{Cue, CueId, CueRegistry, VoiceStealing, sample};
use crate::music::{MusicPlayer, PlayingMusic};
use crate::policy::{AudioPausePolicy, BusOutput};
use crate::spatial::{FollowEntity, SourceLost};

/// A playing instance of a cue.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlayingCue {
    /// The cue being played.
    pub cue: CueId,
    /// The bus the instance plays on.
    pub bus: AudioBus,
    /// The linear volume chosen for this instance, before bus and fade volumes.
    pub base_volume: f32,
    /// Real seconds since startup when the instance started.
    pub started: f64,
}

/// Fades the volume of a sound instance over real time.
#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct Fade {
    /// The multiplier at the start of the fade.
    pub from: f32,
    /// The multiplier at the end of the fade.
    pub to: f32,
    /// The length of the fade in seconds.
    pub duration: f32,
    /// Seconds elapsed since the fade started.
    pub elapsed: f32,
    /// Despawn the instance when the fade completes.
    pub despawn: bool,
}

impl Default for Fade {
    fn default() -> Self {
        Self::new(1.0, 1.0, 0.0)
    }
}

impl Fade {
    /// Fades from one multiplier to another.
    pub fn new(from: f32, to: f32, seconds: f32) -> Self {
        Self {
            from,
            to,
            duration: seconds,
            elapsed: 0.0,
            despawn: false,
        }
    }

    /// Fades from the current multiplier to silence and despawns the instance.
    pub fn out(from: f32, seconds: f32) -> Self {
        Self {
            despawn: true,
            ..Self::new(from, 0.0, seconds)
        }
    }

    /// Returns the current multiplier.
    pub fn value(&self) -> f32 {
        if self.elapsed >= self.duration {
            return self.to;
        }
        self.from + (self.to - self.from) * self.elapsed / self.duration
    }

    /// Returns `true` once the fade has completed.
    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Plays cues and music, and controls buses.
#[derive(SystemParam)]
pub struct Audio<'w, 's> {
    commands: Commands<'w, 's>,
    cues: ResMut<'w, CueRegistry>,
    mixer: ResMut<'w, AudioMixer>,
    music: ResMut<'w, MusicPlayer>,
}

impl<'w, 's> Audio<'w, 's> {
    /// Returns the cue with the given identifier, creating it if needed.
    pub fn register(&mut self, id: impl IntoKey) -> &mut Cue {
        self.cues.register(CueId::new(id))
    }

    /// Plays a registered cue. The instance starts when the returned builder is dropped.
    pub fn play(&mut self, id: impl IntoKey) -> PlayCue<'_, 'w, 's> {
        PlayCue {
            commands: &mut self.commands,
            request: Some(PlayRequest {
                cue: CueId::new(id),
                volume: 1.0,
                pitch: 1.0,
                placement: Placement::Flat,
                looping: false,
                scope: None,
                source_lost: SourceLost::Stop,
            }),
        }
    }

    /// Returns the state of a bus for modification.
    pub fn bus(&mut self, bus: AudioBus) -> &mut BusState {
        self.mixer.bus(bus)
    }

    /// Controls music playback.
    pub fn music(&mut self) -> &mut MusicPlayer {
        &mut self.music
    }

    /// Makes the given camera the only spatial audio listener.
    pub fn listener(&mut self, camera: Entity) {
        self.commands.queue(move |world: &mut World| {
            let listeners: Vec<Entity> = world
                .query_filtered::<Entity, With<SpatialListener>>()
                .iter(world)
                .collect();
            for listener in listeners {
                world.entity_mut(listener).remove::<SpatialListener>();
            }
            world.entity_mut(camera).insert(SpatialListener::default());
        });
    }
}

#[derive(Clone, Copy, Debug)]
enum Placement {
    Flat,
    Point(Vec3),
    Entity(Entity),
}

/// Configures a cue instance. Returned by [`Audio::play`]; plays when dropped.
pub struct PlayCue<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    request: Option<PlayRequest>,
}

impl PlayCue<'_, '_, '_> {
    fn request(&mut self) -> &mut PlayRequest {
        self.request.as_mut().expect("the request is taken only on drop")
    }

    /// Multiplies the volume of this instance.
    pub fn volume(mut self, volume: f32) -> Self {
        self.request().volume *= volume;
        self
    }

    /// Multiplies the playback speed of this instance.
    pub fn pitch(mut self, pitch: f32) -> Self {
        self.request().pitch *= pitch;
        self
    }

    /// Plays spatially, following an entity's position.
    pub fn at_entity(mut self, entity: Entity) -> Self {
        self.request().placement = Placement::Entity(entity);
        self
    }

    /// Plays spatially at a fixed position.
    pub fn at_point(mut self, point: Vec3) -> Self {
        self.request().placement = Placement::Point(point);
        self
    }

    /// Sets what happens when the followed entity despawns.
    pub fn on_source_lost(mut self, policy: SourceLost) -> Self {
        self.request().source_lost = policy;
        self
    }

    /// Repeats until stopped.
    pub fn looping(mut self) -> Self {
        self.request().looping = true;
        self
    }

    /// Stops the instance when the scope closes.
    pub fn in_scope(mut self, scope: Entity) -> Self {
        self.request().scope = Some(scope);
        self
    }
}

impl Drop for PlayCue<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            self.commands.queue(request);
        }
    }
}

struct PlayRequest {
    cue: CueId,
    volume: f32,
    pitch: f32,
    placement: Placement,
    looping: bool,
    scope: Option<Entity>,
    source_lost: SourceLost,
}

impl Command for PlayRequest {
    type Out = ();

    fn apply(self, world: &mut World) {
        let now = world.resource::<Time<Real>>().elapsed_secs_f64();
        let Some(cue) = world.resource::<CueRegistry>().get(self.cue) else {
            warn!("Audio cue {:?} is not registered", self.cue);
            return;
        };
        if cue
            .last_played
            .is_some_and(|last| now - last < cue.cooldown as f64)
        {
            return;
        }
        let (bus, max_instances, stealing) = (cue.bus, cue.max_instances, cue.stealing);
        let playing: Vec<(Entity, PlayingCue)> = world
            .query::<(Entity, &PlayingCue)>()
            .iter(world)
            .filter(|(_, playing)| playing.cue == self.cue)
            .map(|(entity, playing)| (entity, *playing))
            .collect();
        if playing.len() >= max_instances {
            let victim = match stealing {
                VoiceStealing::Oldest => playing
                    .iter()
                    .min_by(|a, b| a.1.started.total_cmp(&b.1.started)),
                VoiceStealing::Quietest => playing
                    .iter()
                    .min_by(|a, b| a.1.base_volume.total_cmp(&b.1.base_volume)),
                VoiceStealing::Reject => None,
            };
            let Some((victim, _)) = victim else {
                return;
            };
            world.despawn(*victim);
        }

        let (bus_volume, paused) = world.resource::<AudioPausePolicy>().output(
            world.resource::<AudioMixer>(),
            bus,
            world.resource::<PauseState>().is_paused(),
            world.resource::<LifecycleState>().foreground,
        );
        let mut registry = world.resource_mut::<CueRegistry>();
        let cue = registry.cues.get_mut(&self.cue).expect("checked above");
        let Some(source) = cue.next_variant() else {
            warn!("Audio cue {:?} has no sources", self.cue);
            return;
        };
        cue.last_played = Some(now);
        let base_volume = sample(&cue.volume) * self.volume;
        let speed = sample(&cue.pitch) * self.pitch;

        let transform = match self.placement {
            Placement::Flat => Transform::default(),
            Placement::Point(point) => Transform::from_translation(point),
            Placement::Entity(target) => world
                .get::<GlobalTransform>(target)
                .map(GlobalTransform::compute_transform)
                .unwrap_or_default(),
        };
        let mut instance = world.spawn((
            AudioPlayer(source),
            PlaybackSettings {
                mode: if self.looping {
                    PlaybackMode::Loop
                } else {
                    PlaybackMode::Despawn
                },
                volume: Volume::Linear(base_volume * bus_volume),
                speed,
                paused,
                spatial: !matches!(self.placement, Placement::Flat),
                ..PlaybackSettings::ONCE
            },
            PlayingCue {
                cue: self.cue,
                bus,
                base_volume,
                started: now,
            },
            transform,
        ));
        if let Placement::Entity(target) = self.placement {
            instance.insert(FollowEntity {
                target,
                lost: self.source_lost,
            });
        }
        if let Some(scope) = self.scope {
            instance.insert(OwnedBy(scope));
        }
    }
}

pub(crate) fn advance_fades(
    time: Res<Time<Real>>,
    mut fades: Query<(Entity, &mut Fade)>,
    mut commands: Commands,
) {
    for (entity, mut fade) in &mut fades {
        fade.elapsed += time.delta_secs();
        if fade.is_finished() && fade.despawn {
            commands.entity(entity).try_despawn();
        }
    }
}

/// Pushes bus, fade, duck, and pause state to every playing sink.
pub(crate) fn apply_audio_output(
    output: BusOutput,
    global: Res<GlobalVolume>,
    music: Res<MusicPlayer>,
    mut sounds: Query<(
        AnyOf<(&PlayingCue, &PlayingMusic)>,
        Option<&Fade>,
        Option<&mut AudioSink>,
        Option<&mut SpatialAudioSink>,
    )>,
) {
    for ((cue, track), fade, sink, spatial_sink) in &mut sounds {
        let (bus, gain) = match (cue, track) {
            (Some(cue), _) => (cue.bus, cue.base_volume),
            (_, Some(track)) => (track.bus, track.base_volume * music.duck_gain()),
            _ => unreachable!("AnyOf matches at least one component"),
        };
        let (bus_volume, paused) = output.get(bus);
        let volume =
            Volume::Linear(gain * bus_volume * fade.map_or(1.0, Fade::value)) * global.volume;
        if let Some(mut sink) = sink {
            drive_sink(&mut *sink, volume, paused);
        }
        if let Some(mut sink) = spatial_sink {
            drive_sink(&mut *sink, volume, paused);
        }
    }
}

fn drive_sink(sink: &mut impl AudioSinkPlayback, volume: Volume, paused: bool) {
    sink.set_volume(volume);
    if paused && !sink.is_paused() {
        sink.pause();
    } else if !paused && sink.is_paused() {
        sink.play();
    }
}
