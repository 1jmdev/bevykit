//! Named sound cues.
//!
//! A cue describes how a sound plays: its source variants, bus, volume and pitch ranges,
//! cooldown, and instance limit. Games register cues once and play them by identifier, using
//! strings or their own enums.
//!
//! ```ignore
//! fn register_sounds(mut audio: Audio, sounds: Res<SoundAssets>) {
//!     audio
//!         .register(SoundId::Confirm)
//!         .source(sounds.confirm.clone())
//!         .bus(AudioBus::Ui)
//!         .max_instances(4);
//! }
//! ```

use std::ops::RangeInclusive;

use bevy::audio::AudioSource;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevykit_core::define_key;

use crate::bus::AudioBus;

define_key!(
    /// Identifies a registered sound cue.
    CueId
);

/// What happens when a cue is played while `max_instances` of it are already playing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Reflect)]
pub enum VoiceStealing {
    /// Stop the instance that started first.
    #[default]
    Oldest,
    /// Stop the instance with the lowest volume.
    Quietest,
    /// Do not play the new instance.
    Reject,
}

/// How a cue plays.
#[derive(Clone, Debug)]
pub struct Cue {
    /// Source variants; one is chosen at random, never the same twice in a row.
    pub sources: Vec<Handle<AudioSource>>,
    /// The bus the cue plays on.
    pub bus: AudioBus,
    /// Linear volume range sampled for each instance.
    pub volume: RangeInclusive<f32>,
    /// Playback speed range sampled for each instance; changes pitch.
    pub pitch: RangeInclusive<f32>,
    /// Minimum real seconds between two plays.
    pub cooldown: f32,
    /// Maximum instances playing at once.
    pub max_instances: usize,
    /// The policy applied when `max_instances` is reached.
    pub stealing: VoiceStealing,
    pub(crate) last_variant: Option<usize>,
    pub(crate) last_played: Option<f64>,
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            bus: AudioBus::Effects,
            volume: 1.0..=1.0,
            pitch: 1.0..=1.0,
            cooldown: 0.0,
            max_instances: usize::MAX,
            stealing: VoiceStealing::Oldest,
            last_variant: None,
            last_played: None,
        }
    }
}

impl Cue {
    /// Adds a source variant.
    pub fn source(&mut self, source: Handle<AudioSource>) -> &mut Self {
        self.sources.push(source);
        self
    }

    /// Adds several source variants.
    pub fn sources(&mut self, sources: impl IntoIterator<Item = Handle<AudioSource>>) -> &mut Self {
        self.sources.extend(sources);
        self
    }

    /// Sets the bus.
    pub fn bus(&mut self, bus: AudioBus) -> &mut Self {
        self.bus = bus;
        self
    }

    /// Sets a fixed linear volume.
    pub fn volume(&mut self, volume: f32) -> &mut Self {
        self.volume = volume..=volume;
        self
    }

    /// Randomizes the linear volume of each instance within a range.
    pub fn volume_range(&mut self, volume: RangeInclusive<f32>) -> &mut Self {
        self.volume = volume;
        self
    }

    /// Sets a fixed playback speed.
    pub fn pitch(&mut self, pitch: f32) -> &mut Self {
        self.pitch = pitch..=pitch;
        self
    }

    /// Randomizes the playback speed of each instance within a range.
    pub fn pitch_range(&mut self, pitch: RangeInclusive<f32>) -> &mut Self {
        self.pitch = pitch;
        self
    }

    /// Sets the minimum real seconds between two plays.
    pub fn cooldown(&mut self, seconds: f32) -> &mut Self {
        self.cooldown = seconds;
        self
    }

    /// Limits the instances playing at once.
    pub fn max_instances(&mut self, count: usize) -> &mut Self {
        self.max_instances = count;
        self
    }

    /// Sets the policy applied when `max_instances` is reached.
    pub fn stealing(&mut self, stealing: VoiceStealing) -> &mut Self {
        self.stealing = stealing;
        self
    }

    pub(crate) fn next_variant(&mut self) -> Option<Handle<AudioSource>> {
        let index = pick_index(self.sources.len(), self.last_variant)?;
        self.last_variant = Some(index);
        Some(self.sources[index].clone())
    }
}

/// Picks a random index below `len`, avoiding `last` when there is another choice.
pub(crate) fn pick_index(len: usize, last: Option<usize>) -> Option<usize> {
    match (len, last) {
        (0, _) => None,
        (1, _) => Some(0),
        (_, None) => Some(fastrand::usize(..len)),
        (_, Some(last)) => {
            let index = fastrand::usize(..len - 1);
            Some(if index >= last { index + 1 } else { index })
        }
    }
}

/// Samples a value uniformly from a range.
pub(crate) fn sample(range: &RangeInclusive<f32>) -> f32 {
    range.start() + (range.end() - range.start()) * fastrand::f32()
}

/// Every registered cue.
#[derive(Resource, Default, Debug)]
pub struct CueRegistry {
    pub(crate) cues: HashMap<CueId, Cue>,
}

impl CueRegistry {
    /// Returns the cue with the given identifier, creating it if needed.
    pub fn register(&mut self, id: CueId) -> &mut Cue {
        self.cues.entry(id).or_default()
    }

    /// Returns a registered cue.
    pub fn get(&self, id: CueId) -> Option<&Cue> {
        self.cues.get(&id)
    }
}
