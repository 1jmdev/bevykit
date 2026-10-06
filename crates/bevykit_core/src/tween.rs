//! Tweens: short animations of component values.
//!
//! A tween drives a value of a component from one state to another over time through an
//! easing curve. Each tween belongs to a *channel* (for example a transform's translation);
//! starting a tween on a channel that is already animating follows an explicit
//! [`TweenConflict`] policy.
//!
//! ```ignore
//! fn show_panel(mut tweens: Tweens, panel: Single<Entity, With<Panel>>) {
//!     tweens
//!         .entity(*panel)
//!         .scale(Vec3::splat(0.9)..=Vec3::ONE)
//!         .duration(0.2)
//!         .ease(EaseFunction::CubicOut)
//!         .replace_existing();
//! }
//! ```
//!
//! Any component can be tweened by registering it with
//! [`TweenAppExt::register_tweenable`] and supplying a [`TweenLens`].

use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::RangeInclusive;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::schedule::KitSystems;

/// Applies an interpolated value to a component.
pub trait TweenLens<C>: Send + Sync + 'static {
    /// Writes the value at `progress`, where `0.0` is the start and `1.0` the end. Eased
    /// progress may overshoot the range.
    fn apply(&mut self, target: &mut C, progress: f32);

    /// Called once when the tween starts, before the first [`apply`](Self::apply), so lenses
    /// that animate from the current value can capture it.
    fn start(&mut self, target: &C) {
        let _ = target;
    }
}

/// A lens built from a closure.
pub struct FnLens<C, F>(F, PhantomData<fn(&mut C)>);

impl<C, F> FnLens<C, F>
where
    F: FnMut(&mut C, f32) + Send + Sync + 'static,
{
    /// Wraps a closure receiving the component and the eased progress.
    pub fn new(apply: F) -> Self {
        Self(apply, PhantomData)
    }
}

impl<C: 'static, F> TweenLens<C> for FnLens<C, F>
where
    F: FnMut(&mut C, f32) + Send + Sync + 'static,
{
    fn apply(&mut self, target: &mut C, progress: f32) {
        (self.0)(target, progress);
    }
}

/// Animates [`Transform::translation`].
#[derive(Clone, Copy, Debug)]
pub struct TranslationLens {
    /// Start value, or `None` to start from the current value.
    pub from: Option<Vec3>,
    /// End value.
    pub to: Vec3,
}

impl TweenLens<Transform> for TranslationLens {
    fn start(&mut self, target: &Transform) {
        self.from.get_or_insert(target.translation);
    }

    fn apply(&mut self, target: &mut Transform, progress: f32) {
        let from = self.from.unwrap_or(target.translation);
        target.translation = from.lerp(self.to, progress);
    }
}

/// Animates [`Transform::scale`].
#[derive(Clone, Copy, Debug)]
pub struct ScaleLens {
    /// Start value, or `None` to start from the current value.
    pub from: Option<Vec3>,
    /// End value.
    pub to: Vec3,
}

impl TweenLens<Transform> for ScaleLens {
    fn start(&mut self, target: &Transform) {
        self.from.get_or_insert(target.scale);
    }

    fn apply(&mut self, target: &mut Transform, progress: f32) {
        let from = self.from.unwrap_or(target.scale);
        target.scale = from.lerp(self.to, progress);
    }
}

/// Animates [`Transform::rotation`] by spherical interpolation.
#[derive(Clone, Copy, Debug)]
pub struct RotationLens {
    /// Start value, or `None` to start from the current value.
    pub from: Option<Quat>,
    /// End value.
    pub to: Quat,
}

impl TweenLens<Transform> for RotationLens {
    fn start(&mut self, target: &Transform) {
        self.from.get_or_insert(target.rotation);
    }

    fn apply(&mut self, target: &mut Transform, progress: f32) {
        let from = self.from.unwrap_or(target.rotation);
        target.rotation = from.slerp(self.to, progress);
    }
}

/// How a new tween interacts with tweens already running on the same channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TweenConflict {
    /// Stop the running tweens and start immediately.
    #[default]
    Replace,
    /// Run alongside; the most recently started tween is applied last.
    Blend,
    /// Start after the running tweens on the channel finish.
    Queue,
}

/// How a tween repeats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TweenRepeat {
    /// Play once.
    #[default]
    Once,
    /// Play the given number of times in total.
    Times(u32),
    /// Repeat forever.
    Forever,
    /// Play forward then backward, forever.
    PingPong,
}

/// Which clock drives a tween.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TimeDomain {
    /// Real time; keeps running while gameplay is paused. The default for presentation.
    #[default]
    Presentation,
    /// Virtual time; stops while gameplay is paused.
    Gameplay,
}

/// Names the animated property, used to detect conflicts.
pub type TweenChannel = Cow<'static, str>;

struct TweenInstance<C> {
    channel: TweenChannel,
    lens: Box<dyn TweenLens<C>>,
    duration: f32,
    delay: f32,
    elapsed: f32,
    ease: EaseFunction,
    repeat: TweenRepeat,
    domain: TimeDomain,
    completed_cycles: u32,
    started: bool,
    waiting: bool,
}

/// The tweens running on component `C` of an entity. Removed when the last tween finishes.
#[derive(Component)]
pub struct ActiveTweens<C: Component> {
    tweens: Vec<TweenInstance<C>>,
}

impl<C: Component> ActiveTweens<C> {
    /// Returns the number of tweens, including queued ones.
    pub fn len(&self) -> usize {
        self.tweens.len()
    }

    /// Returns `true` if no tweens are running.
    pub fn is_empty(&self) -> bool {
        self.tweens.is_empty()
    }

    /// Returns `true` if a tween is running or queued on the channel.
    pub fn is_animating(&self, channel: &str) -> bool {
        self.tweens.iter().any(|tween| tween.channel == channel)
    }
}

/// Triggered on an entity when a tween finishes.
#[derive(EntityEvent, Clone, Debug)]
pub struct TweenCompleted {
    /// The entity.
    pub entity: Entity,
    /// The finished channel.
    pub channel: TweenChannel,
}

/// System parameter for starting tweens.
#[derive(SystemParam)]
pub struct Tweens<'w, 's> {
    commands: Commands<'w, 's>,
}

impl<'w, 's> Tweens<'w, 's> {
    /// Starts describing tweens for an entity.
    pub fn entity(&mut self, entity: Entity) -> EntityTweens<'_, 'w, 's> {
        EntityTweens {
            commands: &mut self.commands,
            entity,
        }
    }

    /// Stops every tween of component `C` on an entity, leaving values where they are.
    pub fn stop<C: Component<Mutability = bevy::ecs::component::Mutable>>(&mut self, entity: Entity) {
        if let Ok(mut entity) = self.commands.get_entity(entity) {
            entity.remove::<ActiveTweens<C>>();
        }
    }
}

/// Tweens for one entity. Returned by [`Tweens::entity`].
pub struct EntityTweens<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    entity: Entity,
}

impl<'a, 'w, 's> EntityTweens<'a, 'w, 's> {
    /// Returns the entity.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Animates component `C` through a custom lens on the given channel.
    pub fn lens<C>(
        &mut self,
        channel: impl Into<TweenChannel>,
        lens: impl TweenLens<C>,
    ) -> TweenBuilder<'_, 'w, 's, C>
    where
        C: Component<Mutability = bevy::ecs::component::Mutable>,
    {
        TweenBuilder {
            commands: &mut *self.commands,
            entity: self.entity,
            instance: Some(TweenInstance {
                channel: channel.into(),
                lens: Box::new(lens),
                duration: 0.25,
                delay: 0.0,
                elapsed: 0.0,
                ease: EaseFunction::CubicOut,
                repeat: TweenRepeat::Once,
                domain: TimeDomain::Presentation,
                completed_cycles: 0,
                started: false,
                waiting: false,
            }),
            conflict: TweenConflict::Replace,
        }
    }

    /// Animates a closure over component `C`.
    pub fn with<C>(
        &mut self,
        channel: impl Into<TweenChannel>,
        apply: impl FnMut(&mut C, f32) + Send + Sync + 'static,
    ) -> TweenBuilder<'_, 'w, 's, C>
    where
        C: Component<Mutability = bevy::ecs::component::Mutable>,
    {
        self.lens(channel, FnLens::new(apply))
    }

    /// Animates the translation between two values.
    pub fn translation(&mut self, range: RangeInclusive<Vec3>) -> TweenBuilder<'_, 'w, 's, Transform> {
        let (from, to) = range.into_inner();
        self.lens("transform.translation", TranslationLens { from: Some(from), to })
    }

    /// Animates the translation from its current value.
    pub fn translation_to(&mut self, to: Vec3) -> TweenBuilder<'_, 'w, 's, Transform> {
        self.lens("transform.translation", TranslationLens { from: None, to })
    }

    /// Animates the scale between two values.
    pub fn scale(&mut self, range: RangeInclusive<Vec3>) -> TweenBuilder<'_, 'w, 's, Transform> {
        let (from, to) = range.into_inner();
        self.lens("transform.scale", ScaleLens { from: Some(from), to })
    }

    /// Animates the scale from its current value.
    pub fn scale_to(&mut self, to: Vec3) -> TweenBuilder<'_, 'w, 's, Transform> {
        self.lens("transform.scale", ScaleLens { from: None, to })
    }

    /// Animates the rotation between two values.
    pub fn rotation(&mut self, range: RangeInclusive<Quat>) -> TweenBuilder<'_, 'w, 's, Transform> {
        let (from, to) = range.into_inner();
        self.lens("transform.rotation", RotationLens { from: Some(from), to })
    }

    /// Animates the rotation from its current value.
    pub fn rotation_to(&mut self, to: Quat) -> TweenBuilder<'_, 'w, 's, Transform> {
        self.lens("transform.rotation", RotationLens { from: None, to })
    }
}

/// Configures one tween. The tween starts when the builder is dropped.
pub struct TweenBuilder<'a, 'w, 's, C>
where
    C: Component<Mutability = bevy::ecs::component::Mutable>,
{
    commands: &'a mut Commands<'w, 's>,
    entity: Entity,
    instance: Option<TweenInstance<C>>,
    conflict: TweenConflict,
}

impl<C> TweenBuilder<'_, '_, '_, C>
where
    C: Component<Mutability = bevy::ecs::component::Mutable>,
{
    fn instance(&mut self) -> &mut TweenInstance<C> {
        self.instance.as_mut().expect("the tween is configured before it starts")
    }

    /// Sets the duration in seconds.
    pub fn duration(&mut self, seconds: f32) -> &mut Self {
        self.instance().duration = seconds.max(0.0);
        self
    }

    /// Sets the duration.
    pub fn duration_of(&mut self, duration: Duration) -> &mut Self {
        self.duration(duration.as_secs_f32())
    }

    /// Delays the start by `seconds`.
    pub fn delay(&mut self, seconds: f32) -> &mut Self {
        self.instance().delay = seconds.max(0.0);
        self
    }

    /// Sets the easing curve.
    pub fn ease(&mut self, ease: EaseFunction) -> &mut Self {
        self.instance().ease = ease;
        self
    }

    /// Sets how the tween repeats.
    pub fn repeat(&mut self, repeat: TweenRepeat) -> &mut Self {
        self.instance().repeat = repeat;
        self
    }

    /// Uses gameplay time, so the tween stops while gameplay is paused.
    pub fn gameplay_time(&mut self) -> &mut Self {
        self.instance().domain = TimeDomain::Gameplay;
        self
    }

    /// Replaces running tweens on the same channel. The default.
    pub fn replace_existing(&mut self) -> &mut Self {
        self.conflict = TweenConflict::Replace;
        self
    }

    /// Runs alongside running tweens on the same channel.
    pub fn blend(&mut self) -> &mut Self {
        self.conflict = TweenConflict::Blend;
        self
    }

    /// Starts after running tweens on the same channel finish.
    pub fn queue(&mut self) -> &mut Self {
        self.conflict = TweenConflict::Queue;
        self
    }
}

impl<C> Drop for TweenBuilder<'_, '_, '_, C>
where
    C: Component<Mutability = bevy::ecs::component::Mutable>,
{
    fn drop(&mut self) {
        let Some(mut instance) = self.instance.take() else {
            return;
        };
        let conflict = self.conflict;
        let entity = self.entity;
        self.commands.queue(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return;
            };
            if !entity.contains::<C>() {
                warn!(
                    "Tween on {:?} ignored: the entity has no {}",
                    entity.id(),
                    std::any::type_name::<C>()
                );
                return;
            }
            match entity.get_mut::<ActiveTweens<C>>() {
                Some(mut active) => {
                    match conflict {
                        TweenConflict::Replace => {
                            active.tweens.retain(|tween| tween.channel != instance.channel);
                        }
                        TweenConflict::Blend => {}
                        TweenConflict::Queue => {
                            instance.waiting = active
                                .tweens
                                .iter()
                                .any(|tween| tween.channel == instance.channel);
                        }
                    }
                    active.tweens.push(instance);
                }
                None => {
                    entity.insert(ActiveTweens {
                        tweens: vec![instance],
                    });
                }
            }
        });
    }
}

/// Advances the tweens of component `C`.
pub fn animate_tweens<C>(
    mut commands: Commands,
    real: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    mut query: Query<(Entity, &mut ActiveTweens<C>, &mut C)>,
) where
    C: Component<Mutability = bevy::ecs::component::Mutable>,
{
    for (entity, mut active, mut target) in &mut query {
        let active = &mut *active;
        let mut finished_channels: Vec<TweenChannel> = Vec::new();

        for index in 0..active.tweens.len() {
            let tween = &active.tweens[index];
            if tween.waiting {
                let blocked = active.tweens[..index]
                    .iter()
                    .any(|earlier| earlier.channel == tween.channel);
                if blocked {
                    continue;
                }
                active.tweens[index].waiting = false;
            }
            let tween = &mut active.tweens[index];
            let delta = match tween.domain {
                TimeDomain::Presentation => real.delta_secs(),
                TimeDomain::Gameplay => virtual_time.delta_secs(),
            };
            if tween.delay > 0.0 {
                tween.delay -= delta;
                if tween.delay > 0.0 {
                    continue;
                }
            }
            if !tween.started {
                tween.started = true;
                tween.lens.start(&target);
            }
            tween.elapsed += delta;

            let duration = tween.duration.max(f32::EPSILON);
            let mut cycle_done = false;
            while tween.elapsed >= duration {
                tween.elapsed -= duration;
                tween.completed_cycles += 1;
                cycle_done = true;
                let more = match tween.repeat {
                    TweenRepeat::Once => false,
                    TweenRepeat::Times(times) => tween.completed_cycles < times,
                    TweenRepeat::Forever | TweenRepeat::PingPong => true,
                };
                if !more {
                    break;
                }
            }
            let finished = cycle_done
                && match tween.repeat {
                    TweenRepeat::Once => true,
                    TweenRepeat::Times(times) => tween.completed_cycles >= times,
                    TweenRepeat::Forever | TweenRepeat::PingPong => false,
                };

            let linear = if finished {
                1.0
            } else {
                (tween.elapsed / duration).clamp(0.0, 1.0)
            };
            let linear = if tween.repeat == TweenRepeat::PingPong && tween.completed_cycles % 2 == 1
            {
                1.0 - linear
            } else {
                linear
            };
            let eased = tween.ease.sample_clamped(linear);
            tween.lens.apply(&mut target, eased);

            if finished {
                finished_channels.push(tween.channel.clone());
            }
        }

        if !finished_channels.is_empty() {
            for channel in &finished_channels {
                if let Some(position) = active
                    .tweens
                    .iter()
                    .position(|tween| tween.channel == *channel && !tween.waiting)
                {
                    active.tweens.remove(position);
                }
                commands.trigger(TweenCompleted {
                    entity,
                    channel: channel.clone(),
                });
            }
            if active.tweens.is_empty() {
                commands.entity(entity).remove::<ActiveTweens<C>>();
            }
        }
    }
}

/// Registers tweenable components on an [`App`].
pub trait TweenAppExt {
    /// Enables tweens of component `C`. Safe to call more than once.
    fn register_tweenable<C>(&mut self) -> &mut Self
    where
        C: Component<Mutability = bevy::ecs::component::Mutable>;
}

#[derive(Resource, Default)]
struct RegisteredTweenables(Vec<std::any::TypeId>);

impl TweenAppExt for App {
    fn register_tweenable<C>(&mut self) -> &mut Self
    where
        C: Component<Mutability = bevy::ecs::component::Mutable>,
    {
        let type_id = std::any::TypeId::of::<C>();
        let mut registered = self
            .world_mut()
            .get_resource_or_insert_with(RegisteredTweenables::default);
        if registered.0.contains(&type_id) {
            return self;
        }
        registered.0.push(type_id);
        self.add_systems(
            PostUpdate,
            animate_tweens::<C>.in_set(KitSystems::Presentation),
        )
    }
}
