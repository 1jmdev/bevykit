//! The [`Animations`] system parameter.
//!
//! Clips are played by name on a scene root that has an
//! [`AnimationLibrary`](crate::clips::AnimationLibrary). Playing the clip that is already
//! running on a layer keeps it running, so systems can request the desired clip every frame.
//! Layer `0` blends through [`AnimationTransitions`]; higher layers play on top of it, for
//! upper-body or additive clips.
//!
//! ```ignore
//! fn animate(mut animations: Animations, characters: Query<(Entity, &Velocity)>) {
//!     for (character, velocity) in &characters {
//!         let clip = if velocity.0.length() > 0.1 { "Run" } else { "Idle" };
//!         animations.on(character).play(clip).repeat().blend_in(0.15);
//!     }
//! }
//! ```

use std::time::Duration;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::animation::transition::AnimationTransitions;
use bevy::animation::{AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::clips::{AnimationLibrary, LibraryClip};
use crate::markers::AnimationMarkers;

pub(crate) type MarkerFn = Box<dyn Fn(&mut Commands) + Send + Sync>;

/// The clip playing on one layer of a character.
pub struct LayerPlayback {
    /// The clip name.
    pub clip: String,
    /// Seconds the clip has played, scaled by speed.
    pub elapsed: f32,
    /// `true` once a clip played with `.once()` has reached its end or the clip was stopped.
    pub finished: bool,
    pub(crate) node: AnimationNodeIndex,
    pub(crate) duration: f32,
    pub(crate) repeat: bool,
    pub(crate) blend_in: f32,
    pub(crate) weight: f32,
    pub(crate) position: f32,
    pub(crate) markers: Vec<(f32, MarkerFn)>,
}

/// The clips playing on a character, by layer.
#[derive(Component, Default)]
pub struct AnimationPlayback {
    pub(crate) layers: Vec<Option<LayerPlayback>>,
}

impl AnimationPlayback {
    /// Returns the clip playing on a layer.
    pub fn layer(&self, layer: usize) -> Option<&LayerPlayback> {
        self.layers.get(layer)?.as_ref()
    }

    /// Returns the name of the clip on layer `0`.
    pub fn clip(&self) -> Option<&str> {
        self.layer(0).map(|layer| layer.clip.as_str())
    }

    /// Returns the elapsed seconds of the clip on layer `0`.
    pub fn elapsed(&self) -> f32 {
        self.layer(0).map_or(0.0, |layer| layer.elapsed)
    }

    /// Returns `true` once the clip on layer `0` has finished.
    pub fn finished(&self) -> bool {
        self.layer(0).is_some_and(|layer| layer.finished)
    }
}

/// Plays animation clips by name.
#[derive(SystemParam)]
pub struct Animations<'w, 's> {
    commands: Commands<'w, 's>,
}

impl<'w, 's> Animations<'w, 's> {
    /// Selects the character, a scene root with an animation library.
    pub fn on(&mut self, character: Entity) -> CharacterAnimations<'_, 'w, 's> {
        CharacterAnimations {
            commands: &mut self.commands,
            character,
        }
    }
}

/// Controls the animations of one character. Returned by [`Animations::on`].
pub struct CharacterAnimations<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    character: Entity,
}

impl<'a, 'w, 's> CharacterAnimations<'a, 'w, 's> {
    /// Plays a clip once on layer `0`. The clip starts when the returned builder is dropped.
    pub fn play(self, clip: impl Into<String>) -> PlayAnimation<'a, 'w, 's> {
        PlayAnimation {
            commands: self.commands,
            request: Some(PlayRequest {
                character: self.character,
                clip: clip.into(),
                repeat: false,
                blend_in: 0.0,
                speed: 1.0,
                seek: None,
                layer: 0,
                markers: Vec::new(),
            }),
        }
    }

    /// Stops every clip on every layer.
    pub fn stop(self) {
        let character = self.character;
        self.commands.queue(move |world: &mut World| {
            let Some(player) = world.get::<AnimationLibrary>(character).map(AnimationLibrary::player)
            else {
                return;
            };
            if let Some(mut player) = world.get_mut::<AnimationPlayer>(player) {
                player.stop_all();
            }
            if let Some(mut playback) = world.get_mut::<AnimationPlayback>(character) {
                playback.layers.clear();
            }
        });
    }
}

/// Configures a clip. Returned by [`CharacterAnimations::play`]; plays when dropped.
pub struct PlayAnimation<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    request: Option<PlayRequest>,
}

impl PlayAnimation<'_, '_, '_> {
    fn request(&mut self) -> &mut PlayRequest {
        self.request.as_mut().expect("the request is taken only on drop")
    }

    /// Repeats the clip until another clip replaces it.
    pub fn repeat(mut self) -> Self {
        self.request().repeat = true;
        self
    }

    /// Plays the clip once and triggers [`AnimationFinished`](crate::markers::AnimationFinished)
    /// at its end. This is the default.
    pub fn once(mut self) -> Self {
        self.request().repeat = false;
        self
    }

    /// Fades the clip in, and the previous clip on layer `0` out, over the given seconds.
    pub fn blend_in(mut self, seconds: f32) -> Self {
        self.request().blend_in = seconds;
        self
    }

    /// Sets the playback speed.
    pub fn speed(mut self, speed: f32) -> Self {
        self.request().speed = speed;
        self
    }

    /// Starts the clip at the given seconds, restarting it if it is already playing.
    pub fn seek(mut self, seconds: f32) -> Self {
        self.request().seek = Some(seconds);
        self
    }

    /// Plays on another layer, on top of layer `0`.
    pub fn layer(mut self, layer: usize) -> Self {
        self.request().layer = layer;
        self
    }

    /// Triggers `event` each time playback passes the named marker of this clip, as listed in
    /// [`AnimationMarkers`].
    pub fn notify_at<E>(mut self, marker: impl Into<String>, event: E) -> Self
    where
        E: for<'t> Event<Trigger<'t>: Default> + Clone,
    {
        self.request().markers.push((
            marker.into(),
            Box::new(move |commands: &mut Commands| commands.trigger(event.clone())),
        ));
        self
    }
}

impl Drop for PlayAnimation<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            self.commands.queue(request);
        }
    }
}

struct PlayRequest {
    character: Entity,
    clip: String,
    repeat: bool,
    blend_in: f32,
    speed: f32,
    seek: Option<f32>,
    layer: usize,
    markers: Vec<(String, MarkerFn)>,
}

impl Command for PlayRequest {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(library) = world.get::<AnimationLibrary>(self.character) else {
            warn!("{} has no animation library", self.character);
            return;
        };
        let player_entity = library.player();
        let Some(LibraryClip { node, clip }) = library.clip(&self.clip).cloned() else {
            warn!("Animation clip {:?} not found on {}", self.clip, self.character);
            return;
        };
        let duration = world
            .resource::<Assets<AnimationClip>>()
            .get(&clip)
            .map_or(0.0, AnimationClip::duration);
        let table = world.resource::<AnimationMarkers>();
        let markers: Vec<(f32, MarkerFn)> = self
            .markers
            .into_iter()
            .filter_map(|(name, notify)| {
                let time = table.get(&self.clip, &name);
                if time.is_none() {
                    warn!("Animation marker {name:?} is not defined for clip {:?}", self.clip);
                }
                Some((time?, notify))
            })
            .collect();

        let mut playback = world
            .get_mut::<AnimationPlayback>(self.character)
            .expect("inserted with the animation library");
        if playback.layers.len() <= self.layer {
            playback.layers.resize_with(self.layer + 1, || None);
        }
        let previous = playback.layers[self.layer]
            .as_ref()
            .map(|layer| (layer.node, layer.finished));
        let replay = previous != Some((node, false)) || self.seek.is_some();

        let Ok((mut player, mut transitions)) = world
            .query::<(&mut AnimationPlayer, &mut AnimationTransitions)>()
            .get_mut(world, player_entity)
        else {
            return;
        };
        let active = match (replay, self.layer) {
            (false, _) => player.play(node),
            (true, 0) => transitions.play(
                &mut player,
                node,
                Duration::from_secs_f32(self.blend_in),
            ),
            (true, _) => {
                if let Some((previous, _)) = previous.filter(|(previous, _)| *previous != node) {
                    player.stop(previous);
                }
                player.start(node)
            }
        };
        active
            .set_repeat(if self.repeat {
                RepeatAnimation::Forever
            } else {
                RepeatAnimation::Never
            })
            .set_speed(self.speed);
        let weight = if replay && self.blend_in > 0.0 { 0.0 } else { 1.0 };
        if replay {
            active.set_weight(weight);
            if let Some(seek) = self.seek {
                active.set_seek_time(seek);
            }
        }

        let mut playback = world
            .get_mut::<AnimationPlayback>(self.character)
            .expect("inserted with the animation library");
        let slot = &mut playback.layers[self.layer];
        if let (false, Some(layer)) = (replay, slot.as_mut()) {
            layer.repeat = self.repeat;
            return;
        }
        *slot = Some(LayerPlayback {
            clip: self.clip,
            elapsed: 0.0,
            finished: false,
            node,
            duration,
            repeat: self.repeat,
            blend_in: self.blend_in,
            weight,
            position: self.seek.unwrap_or(0.0).next_down(),
            markers,
        });
    }
}
