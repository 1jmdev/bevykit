//! Countdowns to wall-clock deadlines.
//!
//! A countdown refreshes itself whenever its displayed text would change, even when nothing
//! else in the world changes, because the deadline service samples the clock every frame.

use std::time::Duration;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevykit_core::deadline::{Deadline, Deadlines};

use crate::binding::{WidgetBinding, add_binding};
use crate::builder::{UiBuilder, WidgetBuilder};
use crate::text::ThemedText;

/// How a countdown shows the remaining time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum CountdownFormat {
    /// `1:05:09` or `5:09`.
    #[default]
    Clock,
    /// The largest two units, such as `2h 5m`.
    Units,
}

/// Shows the time remaining until a deadline.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
#[require(Text)]
pub struct Countdown {
    /// The deadline, or `None` to show nothing.
    pub deadline: Option<Deadline>,
    /// The display format.
    pub format: CountdownFormat,
}

/// Builder for a countdown. Returned by [`UiBuilder::countdown`].
pub struct CountdownBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for CountdownBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl CountdownBuilder<'_> {
    /// Counts down to a deadline.
    pub fn deadline(&mut self, deadline: Deadline) -> &mut Self {
        self.entity
            .entry::<Countdown>()
            .and_modify(move |mut countdown| countdown.deadline = Some(deadline));
        self
    }

    /// Sets the display format.
    pub fn format(&mut self, format: CountdownFormat) -> &mut Self {
        self.entity
            .entry::<Countdown>()
            .and_modify(move |mut countdown| countdown.format = format);
        self
    }

    /// Counts down to a deadline read from resource `R`.
    pub fn bind_resource<R: Resource>(
        &mut self,
        read: impl Fn(&R) -> Option<Deadline> + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            &mut self.entity,
            WidgetBinding::resource(read, |entity, deadline: Option<Deadline>| {
                if let Some(mut countdown) = entity.get_mut::<Countdown>() {
                    countdown.deadline = deadline;
                }
            }),
        );
        self
    }
}

fn format_clock(remaining: Duration) -> String {
    let total = remaining.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_units(remaining: Duration) -> String {
    let total = remaining.as_secs();
    let units = [
        ("d", total / 86_400),
        ("h", (total / 3600) % 24),
        ("m", (total / 60) % 60),
        ("s", total % 60),
    ];
    let first = units.iter().position(|(_, value)| *value > 0).unwrap_or(3);
    units
        .iter()
        .skip(first)
        .take(2)
        .map(|(suffix, value)| format!("{value}{suffix}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(feature = "localization")]
type LocaleParam<'w> = Option<Res<'w, bevykit_data::localization::Locale>>;

pub(crate) fn update_countdowns(
    deadlines: Res<Deadlines>,
    #[cfg(feature = "localization")] locale: LocaleParam,
    mut countdowns: Query<(&Countdown, &mut Text)>,
) {
    for (countdown, mut text) in &mut countdowns {
        let Some(deadline) = countdown.deadline else {
            if !text.0.is_empty() {
                text.0.clear();
            }
            continue;
        };
        let remaining = deadlines.remaining(&deadline);
        #[cfg(feature = "localization")]
        let formatted = match (&locale, countdown.format) {
            (Some(locale), CountdownFormat::Clock) => locale.format_duration(remaining),
            (Some(locale), CountdownFormat::Units) => locale.format_duration_long(remaining),
            (None, CountdownFormat::Clock) => format_clock(remaining),
            (None, CountdownFormat::Units) => format_units(remaining),
        };
        #[cfg(not(feature = "localization"))]
        let formatted = match countdown.format {
            CountdownFormat::Clock => format_clock(remaining),
            CountdownFormat::Units => format_units(remaining),
        };
        if text.0 != formatted {
            text.0 = formatted;
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a countdown.
    pub fn countdown(&mut self) -> CountdownBuilder<'_> {
        CountdownBuilder {
            entity: self.spawn((Countdown::default(), ThemedText::body(), Pickable::IGNORE)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_remaining_time() {
        assert_eq!(format_clock(Duration::from_secs(65)), "1:05");
        assert_eq!(format_clock(Duration::from_secs(3_725)), "1:02:05");
        assert_eq!(format_units(Duration::from_secs(90_061)), "1d 1h");
        assert_eq!(format_units(Duration::ZERO), "0s");
    }
}
