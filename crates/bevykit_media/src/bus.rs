//! Volume buses.
//!
//! Every sound plays on a bus. The [`AudioMixer`] holds the volume, mute, and pause state of
//! each bus, and the effective volume of a bus is its own volume multiplied by the volume of
//! [`AudioBus::Master`]. Changes reach every playing sink on the same frame.
//!
//! ```ignore
//! fn apply_settings(settings: Res<GameSettings>, mut mixer: ResMut<AudioMixer>) {
//!     if settings.is_changed() {
//!         mixer.bus(AudioBus::Music).set_volume(settings.music_volume);
//!     }
//! }
//! ```

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevykit_core::key::{IntoKey, Key};

/// Identifies a volume bus. Every bus other than [`AudioBus::Master`] is scaled by it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Reflect)]
pub enum AudioBus {
    /// The parent of every other bus.
    Master,
    /// Background music.
    Music,
    /// Gameplay sound effects.
    #[default]
    Effects,
    /// Interface sounds.
    Ui,
    /// Dialogue and narration.
    Voice,
    /// A game-defined bus.
    Custom(Key),
}

impl AudioBus {
    /// Creates a game-defined bus from a string or game value.
    pub fn new(name: impl IntoKey) -> Self {
        Self::Custom(name.into_key())
    }
}

/// The volume, mute, and pause state of one bus.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub struct BusState {
    /// Linear volume, where `1.0` is the original level.
    pub volume: f32,
    /// Silences the bus without changing its volume.
    pub muted: bool,
    /// Pauses every sound on the bus.
    pub paused: bool,
}

impl Default for BusState {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
            paused: false,
        }
    }
}

impl BusState {
    /// Sets the linear volume.
    pub fn set_volume(&mut self, volume: f32) -> &mut Self {
        self.volume = volume;
        self
    }

    /// Mutes or unmutes the bus.
    pub fn mute(&mut self, muted: bool) -> &mut Self {
        self.muted = muted;
        self
    }

    /// Pauses every sound on the bus.
    pub fn pause(&mut self) -> &mut Self {
        self.paused = true;
        self
    }

    /// Resumes the sounds on the bus.
    pub fn resume(&mut self) -> &mut Self {
        self.paused = false;
        self
    }

    /// Returns the volume, or zero while muted.
    pub fn gain(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }
}

/// The state of every bus. Buses that were never configured play at full volume.
#[derive(Resource, Default, Debug)]
pub struct AudioMixer {
    buses: HashMap<AudioBus, BusState>,
}

impl AudioMixer {
    /// Returns the state of a bus for modification.
    pub fn bus(&mut self, bus: AudioBus) -> &mut BusState {
        self.buses.entry(bus).or_default()
    }

    /// Returns the state of a bus.
    pub fn state(&self, bus: AudioBus) -> BusState {
        self.buses.get(&bus).copied().unwrap_or_default()
    }

    /// Returns the volume of a bus multiplied by the master volume.
    pub fn effective_volume(&self, bus: AudioBus) -> f32 {
        match bus {
            AudioBus::Master => self.state(bus).gain(),
            _ => self.state(bus).gain() * self.state(AudioBus::Master).gain(),
        }
    }

    /// Returns `true` if the bus or the master bus is paused.
    pub fn is_paused(&self, bus: AudioBus) -> bool {
        self.state(bus).paused || self.state(AudioBus::Master).paused
    }
}
