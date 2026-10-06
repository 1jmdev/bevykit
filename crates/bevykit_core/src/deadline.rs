//! Persistent wall-clock deadlines, separate from simulation timers.
//!
//! A [`Deadline`] is an absolute point in wall-clock time. Because it does not depend on the
//! game running, it survives restarts: a job started before the game closed completes while
//! the player is away, and the countdown shows the right value when they return.
//!
//! Deadlines read time from a [`WallClock`], which the game may replace (for example with a
//! server-synchronized clock). [`ClockChangePolicy`] decides how to treat a clock that moves
//! backwards, which happens when the player changes the device time.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A point in wall-clock time, in milliseconds since the Unix epoch.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize, Reflect,
)]
#[serde(transparent)]
pub struct WallTime(pub u64);

impl WallTime {
    /// Returns the time as milliseconds since the Unix epoch.
    pub const fn as_millis(&self) -> u64 {
        self.0
    }

    /// Adds a duration, saturating at the maximum representable time.
    pub fn saturating_add(self, duration: Duration) -> Self {
        let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        Self(self.0.saturating_add(millis))
    }

    /// Returns the duration from `earlier` to `self`, or zero if `earlier` is later.
    pub fn saturating_duration_since(self, earlier: WallTime) -> Duration {
        Duration::from_millis(self.0.saturating_sub(earlier.0))
    }
}

/// A source of wall-clock time.
pub trait WallClock: Send + Sync + 'static {
    /// Returns the current wall-clock time.
    fn now(&self) -> WallTime;
}

/// The operating system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now(&self) -> WallTime {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        WallTime(u64::try_from(millis).unwrap_or(u64::MAX))
    }
}

/// Determines how the deadline service reacts when the clock moves backwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum ClockChangePolicy {
    /// Always trust the clock, even when it moves backwards.
    Trust,
    /// Never let observed time move backwards. Progress made before a backwards change is
    /// preserved, and the clock must catch up before time advances again.
    #[default]
    Monotonic,
}

/// A wall-clock deadline that can be persisted and displayed as a countdown.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize, Reflect,
)]
pub struct Deadline {
    /// When the deadline was created.
    pub start: WallTime,
    /// When the deadline expires.
    pub end: WallTime,
}

impl Deadline {
    /// Creates a deadline spanning from `start` to `end`.
    pub fn new(start: WallTime, end: WallTime) -> Self {
        Self {
            start,
            end: end.max(start),
        }
    }

    /// Returns the total duration of the deadline.
    pub fn total(&self) -> Duration {
        self.end.saturating_duration_since(self.start)
    }

    /// Returns the remaining duration at `now`.
    pub fn remaining_at(&self, now: WallTime) -> Duration {
        self.end.saturating_duration_since(now)
    }

    /// Returns `true` if the deadline has passed at `now`.
    pub fn is_expired_at(&self, now: WallTime) -> bool {
        now >= self.end
    }

    /// Returns the elapsed fraction in `0.0..=1.0` at `now`.
    pub fn progress_at(&self, now: WallTime) -> f32 {
        let total = self.end.0.saturating_sub(self.start.0);
        if total == 0 {
            return 1.0;
        }
        let elapsed = now.0.saturating_sub(self.start.0).min(total);
        (elapsed as f64 / total as f64) as f32
    }
}

/// The deadline service: the active clock and the time observed most recently.
#[derive(Resource, Clone)]
pub struct Deadlines {
    clock: Arc<dyn WallClock>,
    policy: ClockChangePolicy,
    observed: WallTime,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self::new(SystemWallClock, ClockChangePolicy::default())
    }
}

impl Deadlines {
    /// Creates a service with a custom clock and policy.
    pub fn new(clock: impl WallClock, policy: ClockChangePolicy) -> Self {
        let observed = clock.now();
        Self {
            clock: Arc::new(clock),
            policy,
            observed,
        }
    }

    /// Replaces the clock source.
    pub fn set_clock(&mut self, clock: impl WallClock) {
        self.clock = Arc::new(clock);
        self.refresh();
    }

    /// Returns the clock change policy.
    pub fn policy(&self) -> ClockChangePolicy {
        self.policy
    }

    /// Sets the clock change policy.
    pub fn set_policy(&mut self, policy: ClockChangePolicy) {
        self.policy = policy;
    }

    /// Returns the time observed at the start of this frame, after applying the policy.
    pub fn now(&self) -> WallTime {
        self.observed
    }

    /// Restores the most recent observed time, typically from save data, so that the
    /// [`ClockChangePolicy::Monotonic`] policy also holds across restarts.
    pub fn restore_observed(&mut self, observed: WallTime) {
        self.observed = self.observed.max(observed);
    }

    /// Samples the clock and applies the change policy.
    pub fn refresh(&mut self) {
        let sampled = self.clock.now();
        self.observed = match self.policy {
            ClockChangePolicy::Trust => sampled,
            ClockChangePolicy::Monotonic => sampled.max(self.observed),
        };
    }

    /// Creates a deadline expiring `duration` from now.
    pub fn after(&self, duration: Duration) -> Deadline {
        Deadline::new(self.observed, self.observed.saturating_add(duration))
    }

    /// Creates a deadline expiring at an absolute time.
    pub fn at(&self, end: WallTime) -> Deadline {
        Deadline::new(self.observed, end)
    }

    /// Returns the remaining duration of a deadline.
    pub fn remaining(&self, deadline: &Deadline) -> Duration {
        deadline.remaining_at(self.observed)
    }

    /// Returns `true` if the deadline has passed.
    pub fn is_expired(&self, deadline: &Deadline) -> bool {
        deadline.is_expired_at(self.observed)
    }

    /// Returns the elapsed fraction of a deadline.
    pub fn progress(&self, deadline: &Deadline) -> f32 {
        deadline.progress_at(self.observed)
    }
}

/// Samples the wall clock once per frame so every system sees a consistent time.
pub fn refresh_deadlines(mut deadlines: ResMut<Deadlines>) {
    deadlines.refresh();
}

/// Tracks a deadline on an entity and triggers [`DeadlineExpired`] once it passes.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Debug)]
pub struct DeadlineTimer {
    /// The tracked deadline.
    pub deadline: Deadline,
    expired: bool,
}

impl DeadlineTimer {
    /// Tracks the given deadline.
    pub fn new(deadline: Deadline) -> Self {
        Self {
            deadline,
            expired: false,
        }
    }

    /// Returns `true` once the deadline has been reported as expired.
    pub fn is_expired(&self) -> bool {
        self.expired
    }
}

/// Triggered on an entity when its [`DeadlineTimer`] expires.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct DeadlineExpired {
    /// The entity whose deadline expired.
    pub entity: Entity,
    /// The deadline that expired.
    pub deadline: Deadline,
}

/// Reports expired [`DeadlineTimer`]s.
pub fn report_expired_deadlines(
    deadlines: Res<Deadlines>,
    mut timers: Query<(Entity, &mut DeadlineTimer)>,
    mut commands: Commands,
) {
    for (entity, mut timer) in &mut timers {
        if timer.expired || !deadlines.is_expired(&timer.deadline) {
            continue;
        }
        timer.expired = true;
        commands.trigger(DeadlineExpired {
            entity,
            deadline: timer.deadline,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[derive(Clone)]
    struct ManualClock(Arc<AtomicU64>);

    impl WallClock for ManualClock {
        fn now(&self) -> WallTime {
            WallTime(self.0.load(Ordering::Relaxed))
        }
    }

    #[test]
    fn monotonic_policy_ignores_backward_changes() {
        let millis = Arc::new(AtomicU64::new(10_000));
        let mut deadlines = Deadlines::new(ManualClock(millis.clone()), ClockChangePolicy::Monotonic);
        let deadline = deadlines.after(Duration::from_secs(5));

        millis.store(12_000, Ordering::Relaxed);
        deadlines.refresh();
        assert_eq!(deadlines.remaining(&deadline), Duration::from_secs(3));

        millis.store(1_000, Ordering::Relaxed);
        deadlines.refresh();
        assert_eq!(deadlines.remaining(&deadline), Duration::from_secs(3));

        millis.store(16_000, Ordering::Relaxed);
        deadlines.refresh();
        assert!(deadlines.is_expired(&deadline));
        assert_eq!(deadlines.progress(&deadline), 1.0);
    }

    #[test]
    fn deadline_round_trips_through_serde() {
        let deadline = Deadline::new(WallTime(5), WallTime(10));
        let encoded = serde_json::to_string(&deadline).unwrap();
        let decoded: Deadline = serde_json::from_str(&encoded).unwrap();
        assert_eq!(deadline, decoded);
    }
}
