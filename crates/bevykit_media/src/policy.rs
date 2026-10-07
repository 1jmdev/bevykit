//! How gameplay pause and the application lifecycle affect each bus.
//!
//! While [`PauseState`](bevykit_core::pause::PauseState) holds any reason, each bus follows its
//! [`BusPausePolicy`]: gameplay effects pause, while music and interface sounds keep playing.
//! When the application moves to the background, every sink pauses before the
//! [`OnBackground`](bevykit_core::platform::OnBackground) schedule returns and resumes in the
//! foreground.

use bevy::ecs::system::SystemParam;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevykit_core::pause::PauseState;
use bevykit_core::platform::LifecycleState;

use crate::bus::{AudioBus, AudioMixer};

/// What a bus does while gameplay is paused.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub enum BusPausePolicy {
    /// Pause with gameplay.
    Pause,
    /// Keep playing.
    Continue,
    /// Keep playing at the given fraction of its volume.
    Duck(f32),
}

/// The pause behavior of every bus.
#[derive(Resource, Debug)]
pub struct AudioPausePolicy {
    buses: HashMap<AudioBus, BusPausePolicy>,
    /// The policy of buses without an explicit entry.
    pub fallback: BusPausePolicy,
    /// Pause every sink while the application is in the background.
    pub pause_in_background: bool,
}

impl Default for AudioPausePolicy {
    fn default() -> Self {
        Self {
            buses: HashMap::from_iter([
                (AudioBus::Master, BusPausePolicy::Continue),
                (AudioBus::Music, BusPausePolicy::Continue),
                (AudioBus::Ui, BusPausePolicy::Continue),
            ]),
            fallback: BusPausePolicy::Pause,
            pause_in_background: true,
        }
    }
}

impl AudioPausePolicy {
    /// Sets the policy of a bus.
    pub fn set(&mut self, bus: AudioBus, policy: BusPausePolicy) -> &mut Self {
        self.buses.insert(bus, policy);
        self
    }

    /// Returns the policy of a bus.
    pub fn get(&self, bus: AudioBus) -> BusPausePolicy {
        self.buses.get(&bus).copied().unwrap_or(self.fallback)
    }

    /// Returns the volume multiplier and paused flag that the mixer, gameplay pause, and the
    /// application lifecycle impose on a bus.
    pub fn output(
        &self,
        mixer: &AudioMixer,
        bus: AudioBus,
        gameplay_paused: bool,
        foreground: bool,
    ) -> (f32, bool) {
        let volume = mixer.effective_volume(bus);
        let paused = mixer.is_paused(bus) || (self.pause_in_background && !foreground);
        match (gameplay_paused, self.get(bus)) {
            (true, BusPausePolicy::Pause) => (volume, true),
            (true, BusPausePolicy::Duck(fraction)) => (volume * fraction, paused),
            _ => (volume, paused),
        }
    }
}

/// Reads the volume multiplier and paused flag of a bus.
#[derive(SystemParam)]
pub struct BusOutput<'w> {
    mixer: Res<'w, AudioMixer>,
    policy: Res<'w, AudioPausePolicy>,
    pause: Res<'w, PauseState>,
    lifecycle: Res<'w, LifecycleState>,
}

impl BusOutput<'_> {
    /// Returns the volume multiplier and paused flag of a bus.
    pub fn get(&self, bus: AudioBus) -> (f32, bool) {
        self.policy
            .output(&self.mixer, bus, self.pause.is_paused(), self.lifecycle.foreground)
    }
}
