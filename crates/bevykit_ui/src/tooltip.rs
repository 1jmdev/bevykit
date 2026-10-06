//! Tooltips shown after hovering a widget, or holding it on touch screens.

use bevy::picking::Pickable;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::ui::GlobalZIndex;
use bevykit_input::prelude::*;

use crate::builder::WidgetBuilder;
use crate::state::WidgetVisuals;
use crate::text::{ThemedText, UiText};
use crate::theme::{TextRole, UiTheme};

/// Text shown near the widget when the player lingers on it.
#[derive(Component, Clone, Debug)]
pub struct Tooltip {
    /// The text.
    pub text: UiText,
}

/// Tooltip timing.
#[derive(Resource, Clone, Debug)]
pub struct TooltipSettings {
    /// Seconds a mouse must hover before the tooltip appears.
    pub hover_delay: f32,
    /// Seconds a touch must be held before the tooltip appears.
    pub hold_delay: f32,
}

impl Default for TooltipSettings {
    fn default() -> Self {
        Self {
            hover_delay: 0.6,
            hold_delay: 0.5,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct ActiveTooltip {
    owner: Option<Entity>,
    lingered: f32,
    shown: Option<Entity>,
}

/// Adds tooltips to widget builders.
pub trait TooltipExt<'a>: WidgetBuilder<'a> {
    /// Shows `text` when the player lingers on the widget.
    fn tooltip(&mut self, text: impl Into<UiText>) -> &mut Self {
        self.insert(Tooltip { text: text.into() })
    }
}

impl<'a, T: WidgetBuilder<'a>> TooltipExt<'a> for T {}

pub(crate) fn update_tooltips(
    router: Res<PointerRouter>,
    settings: Res<TooltipSettings>,
    theme: Res<UiTheme>,
    time: Res<Time<Real>>,
    tooltips: Query<&Tooltip>,
    mut active: ResMut<ActiveTooltip>,
    mut commands: Commands,
) {
    let lingering = router.pointers().find_map(|pointer| {
        let eligible = pointer.id() == PointerId::Mouse && !pointer.is_down()
            || pointer.is_down() && pointer.travel() < 10.0;
        if !eligible {
            return None;
        }
        let owner = pointer
            .hover_targets()
            .iter()
            .find(|entity| tooltips.contains(**entity))?;
        let delay = if pointer.id() == PointerId::Mouse {
            settings.hover_delay
        } else {
            settings.hold_delay
        };
        Some((*owner, pointer.position(), delay))
    });

    let Some((owner, position, delay)) = lingering else {
        if let Some(shown) = active.shown.take() {
            commands.entity(shown).try_despawn();
        }
        active.owner = None;
        active.lingered = 0.0;
        return;
    };

    if active.owner != Some(owner) {
        if let Some(shown) = active.shown.take() {
            commands.entity(shown).try_despawn();
        }
        active.owner = Some(owner);
        active.lingered = 0.0;
    }
    active.lingered += time.delta_secs();
    if active.shown.is_some() || active.lingered < delay {
        return;
    }
    let Ok(tooltip) = tooltips.get(owner) else {
        return;
    };

    let panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(position.x + 12.0),
                top: Val::Px(position.y + 18.0),
                padding: UiRect::axes(theme.gap(1.0), theme.gap(0.5)),
                border: UiRect::all(Val::Px(theme.border_width)),
                border_radius: BorderRadius::all(Val::Px(theme.radius)),
                max_width: Val::Px(320.0),
                ..default()
            },
            WidgetVisuals::Surface,
            GlobalZIndex(i32::MAX - 1),
            Pickable::IGNORE,
        ))
        .id();
    let mut text = commands.spawn((
        ThemedText::role(TextRole::Caption),
        Pickable::IGNORE,
        ChildOf(panel),
    ));
    tooltip.text.clone().insert(&mut text);
    active.shown = Some(panel);
}
