//! Music playlists and transitions.
//!
//! Each [`MusicId`] names a playlist of tracks. Transitions crossfade between two sinks and are
//! driven by real time, so fades continue while gameplay is paused.
//!
//! ```ignore
//! fn enter_forest(mut audio: Audio) {
//!     audio.music().transition_to(MusicId::new(Music::Exploration)).fade(1.5);
//! }
//!
//! fn start_dialogue(mut audio: Audio) {
//!     audio.music().duck(0.6, 0.3);
//! }
//! ```

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackMode, PlaybackSettings,
    Volume,
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevykit_core::define_key;
use bevykit_core::key::IntoKey;

use crate::bus::AudioBus;
use crate::cue::pick_index;
use crate::playback::Fade;
use crate::policy::BusOutput;

define_key!(
    /// Identifies a music playlist.
    MusicId
);

/// The tracks of a playlist and how they follow each other.
#[derive(Clone, Debug)]
pub struct Playlist {
    /// The tracks, played in order unless shuffled.
    pub tracks: Vec<Handle<AudioSource>>,
    /// Play tracks in random order, never the same twice in a row.
    pub shuffle: bool,
    /// Start over after the last track instead of stopping.
    pub repeat: bool,
    /// The bus the playlist plays on.
    pub bus: AudioBus,
    /// Linear volume of every track.
    pub volume: f32,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            shuffle: false,
            repeat: true,
            bus: AudioBus::Music,
            volume: 1.0,
        }
    }
}

impl Playlist {
    /// Adds a track.
    pub fn track(&mut self, track: Handle<AudioSource>) -> &mut Self {
        self.tracks.push(track);
        self
    }

    /// Plays tracks in random order.
    pub fn shuffle(&mut self, shuffle: bool) -> &mut Self {
        self.shuffle = shuffle;
        self
    }

    /// Starts over after the last track instead of stopping.
    pub fn repeat(&mut self, repeat: bool) -> &mut Self {
        self.repeat = repeat;
        self
    }

    /// Sets the bus.
    pub fn bus(&mut self, bus: AudioBus) -> &mut Self {
        self.bus = bus;
        self
    }

    /// Sets the linear volume of every track.
    pub fn volume(&mut self, volume: f32) -> &mut Self {
        self.volume = volume;
        self
    }
}

/// A playing music track.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlayingMusic {
    /// The playlist being played.
    pub music: MusicId,
    /// The index of the track in the playlist.
    pub track: usize,
    /// Tracks played from the playlist since it started, including this one.
    pub plays: usize,
    /// The bus the track plays on.
    pub bus: AudioBus,
    /// The linear volume of the track, before bus, fade, and duck volumes.
    pub base_volume: f32,
}

/// Music playlists and the playing track.
#[derive(Resource, Default, Debug)]
pub struct MusicPlayer {
    playlists: HashMap<MusicId, Playlist>,
    pending: Option<(Option<MusicId>, f32)>,
    current: Option<(MusicId, Entity)>,
    duck: Fade,
}

impl MusicPlayer {
    /// Returns the playlist with the given identifier, creating it if needed.
    pub fn register(&mut self, id: impl IntoKey) -> &mut Playlist {
        self.playlists.entry(MusicId::new(id)).or_default()
    }

    /// Switches to another playlist. Does nothing if it is already playing.
    pub fn transition_to(&mut self, id: impl IntoKey) -> MusicFade<'_> {
        let (_, fade) = self.pending.insert((Some(MusicId::new(id)), 0.0));
        MusicFade(fade)
    }

    /// Stops the music.
    pub fn stop(&mut self) -> MusicFade<'_> {
        let (_, fade) = self.pending.insert((None, 0.0));
        MusicFade(fade)
    }

    /// Lowers the music by `amount` (from `0.0` to `1.0`) over the given seconds, for
    /// dialogue. `duck(0.0, seconds)` restores the full volume.
    pub fn duck(&mut self, amount: f32, seconds: f32) {
        self.duck = Fade::new(self.duck.value(), 1.0 - amount, seconds);
    }

    /// Returns the current duck multiplier.
    pub fn duck_gain(&self) -> f32 {
        self.duck.value()
    }

    /// Returns the playing playlist and the entity of its track.
    pub fn current(&self) -> Option<(MusicId, Entity)> {
        self.current
    }

    fn start(
        &mut self,
        commands: &mut Commands,
        output: &BusOutput,
        id: MusicId,
        previous: Option<usize>,
        plays: usize,
        fade: f32,
    ) {
        let Some(playlist) = self.playlists.get(&id) else {
            warn!("Music {id:?} is not registered");
            return;
        };
        let len = playlist.tracks.len();
        if !playlist.repeat && plays >= len {
            return;
        }
        let track = match (playlist.shuffle, previous) {
            (true, _) => pick_index(len, previous),
            (false, None) => (len > 0).then_some(0),
            (false, Some(previous)) => Some((previous + 1) % len),
        };
        let Some(track) = track else {
            return;
        };
        let (bus_volume, paused) = output.get(playlist.bus);
        let fade = Fade::new(0.0, 1.0, fade);
        let entity = commands
            .spawn((
                AudioPlayer(playlist.tracks[track].clone()),
                PlaybackSettings {
                    mode: if len == 1 && playlist.repeat {
                        PlaybackMode::Loop
                    } else {
                        PlaybackMode::Once
                    },
                    volume: Volume::Linear(
                        playlist.volume * bus_volume * self.duck.value() * fade.value(),
                    ),
                    paused,
                    ..PlaybackSettings::ONCE
                },
                PlayingMusic {
                    music: id,
                    track,
                    plays: plays + 1,
                    bus: playlist.bus,
                    base_volume: playlist.volume,
                },
                fade,
            ))
            .id();
        self.current = Some((id, entity));
    }
}

/// Sets the fade of a music transition. Returned by [`MusicPlayer::transition_to`] and
/// [`MusicPlayer::stop`]; without it, the change is immediate.
pub struct MusicFade<'a>(&'a mut f32);

impl MusicFade<'_> {
    /// Crossfades over the given real seconds.
    pub fn fade(self, seconds: f32) {
        *self.0 = seconds;
    }
}

pub(crate) fn drive_music(
    time: Res<Time<Real>>,
    mut player: ResMut<MusicPlayer>,
    tracks: Query<(&PlayingMusic, Option<&AudioSink>, Option<&Fade>)>,
    output: BusOutput,
    mut commands: Commands,
) {
    player.duck.elapsed += time.delta_secs();
    if let Some((next, fade)) = player.pending.take() {
        if next.is_some() && next == player.current.map(|(id, _)| id) {
            return;
        }
        if let Some((_, entity)) = player.current.take()
            && let Ok((_, _, current)) = tracks.get(entity)
        {
            commands
                .entity(entity)
                .try_insert(Fade::out(current.map_or(1.0, Fade::value), fade));
        }
        if let Some(id) = next {
            player.start(&mut commands, &output, id, None, 0, fade);
        }
        return;
    }
    let Some((id, entity)) = player.current else {
        return;
    };
    let Ok((track, sink, _)) = tracks.get(entity) else {
        player.current = None;
        return;
    };
    if !sink.is_some_and(AudioSinkPlayback::empty) {
        return;
    }
    commands.entity(entity).try_despawn();
    player.current = None;
    player.start(&mut commands, &output, id, Some(track.track), track.plays, 0.0);
}
