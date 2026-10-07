//! Sliders.
//!
//! A slider follows the pointer that pressed it, capturing the pointer so a surrounding scroll
//! view does not scroll at the same time. While focused, left and right adjust the value
//! instead of moving focus.

use std::ops::RangeInclusive;

use bevy::input_focus::InputFocus;
use bevy::picking::Pickable;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevykit_input::prelude::*;

use crate::actions::NavigationRequest;
use crate::binding::{WidgetBinding, add_binding};
use crate::builder::{UiBuilder, WidgetBuilder};
use crate::focus::Focusable;
use crate::geometry::logical_rect;
use crate::interaction::{OnValueChange, ValueChanged};
use crate::state::{WidgetState, WidgetVisuals};
use crate::text::{ThemedText, UiText};

/// A value chosen along a range.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
#[require(Focusable, WidgetState)]
pub struct Slider {
    /// The smallest value.
    pub min: f32,
    /// The largest value.
    pub max: f32,
    /// The increment between values, or `None` for continuous values.
    pub step: Option<f32>,
    /// The current value.
    pub value: f32,
    #[reflect(ignore)]
    dragging: Option<PointerId>,
    fill: Option<Entity>,
    readout: Option<Entity>,
    #[reflect(ignore)]
    format: Option<fn(f32) -> String>,
}

impl Slider {
    fn snap(&self, value: f32) -> f32 {
        let clamped = value.clamp(self.min.min(self.max), self.max.max(self.min));
        match self.step {
            Some(step) if step > 0.0 => {
                let steps = ((clamped - self.min) / step).round();
                (self.min + steps * step).clamp(self.min.min(self.max), self.max.max(self.min))
            }
            _ => clamped,
        }
    }

    /// Returns the value as a fraction of the range.
    pub fn fraction(&self) -> f32 {
        let span = self.max - self.min;
        if span == 0.0 {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }

    fn keyboard_step(&self) -> f32 {
        self.step.unwrap_or((self.max - self.min) / 20.0)
    }
}

/// Builder for a slider. Returned by [`UiBuilder::slider`].
pub struct SliderBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for SliderBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl SliderBuilder<'_> {
    fn modify(&mut self, change: impl FnOnce(&mut Slider) + Send + Sync + 'static) -> &mut Self {
        self.entity.entry::<Slider>().and_modify(move |mut slider| {
            change(&mut slider);
            slider.value = slider.snap(slider.value);
        });
        self
    }

    /// Sets the range of values.
    pub fn range(&mut self, range: RangeInclusive<f32>) -> &mut Self {
        let (min, max) = range.into_inner();
        self.modify(move |slider| {
            slider.min = min;
            slider.max = max;
        })
    }

    /// Sets the increment between values.
    pub fn step(&mut self, step: f32) -> &mut Self {
        self.modify(move |slider| slider.step = Some(step))
    }

    /// Sets the initial value.
    pub fn value(&mut self, value: f32) -> &mut Self {
        self.modify(move |slider| slider.value = value)
    }

    /// Shows the value next to the label, formatted by `format`.
    pub fn show_value(&mut self, format: fn(f32) -> String) -> &mut Self {
        self.modify(move |slider| slider.format = Some(format))
    }

    /// Writes the message produced by `message` whenever the player changes the value.
    pub fn on_change<M: Message>(
        &mut self,
        message: impl Fn(f32) -> M + Send + Sync + 'static,
    ) -> &mut Self {
        self.entity
            .entry::<OnValueChange<f32>>()
            .or_default()
            .and_modify(move |mut callbacks| {
                callbacks.push(move |commands, value| {
                    commands.write_message(message(value));
                });
            });
        self
    }

    /// Follows a value computed from resource `R`, except while the player is dragging.
    pub fn bind_resource<R: Resource>(
        &mut self,
        read: impl Fn(&R) -> f32 + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            &mut self.entity,
            WidgetBinding::resource(read, |entity, value: f32| {
                if let Some(mut slider) = entity.get_mut::<Slider>()
                    && slider.dragging.is_none()
                {
                    slider.value = slider.snap(value);
                }
            }),
        );
        self
    }
}

fn set_value(entity: Entity, slider: &mut Slider, value: f32, commands: &mut Commands) {
    let value = slider.snap(value);
    if value != slider.value {
        slider.value = value;
        commands.trigger(ValueChanged { entity, value });
    }
}

pub(crate) fn drag_sliders(
    mut router: ResMut<PointerRouter>,
    mut sliders: Query<(Entity, &mut Slider, &WidgetState, &ComputedNode, &UiGlobalTransform)>,
    mut commands: Commands,
) {
    for (entity, mut slider, state, node, transform) in &mut sliders {
        if state.disabled {
            slider.dragging = None;
            continue;
        }
        if slider.dragging.is_none()
            && let Some(press) = router.press_on(entity)
            && router.capture(press.id, entity)
        {
            slider.dragging = Some(press.id);
        }
        let Some(id) = slider.dragging else {
            continue;
        };
        let Some(pointer) = router.get(id).filter(|pointer| pointer.owner() == Some(entity))
        else {
            slider.dragging = None;
            continue;
        };
        let rect = logical_rect(node, transform);
        if rect.width() > 0.0 {
            let fraction = ((pointer.position().x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            let value = slider.min + fraction * (slider.max - slider.min);
            set_value(entity, &mut slider, value, &mut commands);
        }
        if pointer.has_ended() {
            slider.dragging = None;
        }
    }
}

/// Adjusts the focused slider with left and right, consuming the navigation request.
pub(crate) fn adjust_focused_slider(
    focus: Res<InputFocus>,
    mut request: ResMut<NavigationRequest>,
    mut sliders: Query<(&mut Slider, &WidgetState)>,
    mut commands: Commands,
) {
    let (Some(direction), Some(entity)) = (request.0, focus.get()) else {
        return;
    };
    let Ok((mut slider, state)) = sliders.get_mut(entity) else {
        return;
    };
    if state.disabled || direction.x == 0.0 {
        return;
    }
    request.0 = None;
    let value = slider.value + slider.keyboard_step() * direction.x.signum();
    set_value(entity, &mut slider, value, &mut commands);
}

pub(crate) fn update_slider_visuals(
    sliders: Query<&Slider, Changed<Slider>>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
) {
    for slider in &sliders {
        if let Some(mut node) = slider.fill.and_then(|fill| nodes.get_mut(fill).ok()) {
            node.width = Val::Percent(slider.fraction() * 100.0);
        }
        if let (Some(readout), Some(format)) = (slider.readout, slider.format)
            && let Ok(mut text) = texts.get_mut(readout)
        {
            text.0 = format(slider.value);
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a labeled slider with a default range of `0.0..=1.0`.
    pub fn slider(&mut self, text: impl Into<UiText>) -> SliderBuilder<'_> {
        let theme = self.theme;
        let column = self
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: theme.gap(0.5),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .id();
        let header = self
            .commands
            .spawn((
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(column),
            ))
            .id();
        let mut label = self
            .commands
            .spawn((ThemedText::body(), Pickable::IGNORE, ChildOf(header)));
        text.into().insert(&mut label);
        let readout = self
            .commands
            .spawn((Text::default(), ThemedText::body(), Pickable::IGNORE, ChildOf(header)))
            .id();

        let track_height = theme.spacing;
        let track = self
            .commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(theme.control_height * 0.6),
                    align_items: AlignItems::Center,
                    ..default()
                },
                WidgetVisuals::Unstyled,
                ChildOf(column),
            ))
            .id();
        let groove = self
            .commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(track_height),
                    border_radius: BorderRadius::all(Val::Px(track_height)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                WidgetVisuals::Track,
                Pickable::IGNORE,
                ChildOf(track),
            ))
            .id();
        let fill = self
            .commands
            .spawn((
                Node {
                    height: Val::Percent(100.0),
                    ..default()
                },
                WidgetVisuals::Fill,
                Pickable::IGNORE,
                ChildOf(groove),
            ))
            .id();
        self.commands.entity(track).insert(Slider {
            min: 0.0,
            max: 1.0,
            step: None,
            value: 0.0,
            dragging: None,
            fill: Some(fill),
            readout: Some(readout),
            format: None,
        });
        SliderBuilder {
            entity: self.commands.entity(track),
        }
    }
}
