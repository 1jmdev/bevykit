//! Widget interaction state and its themed presentation.

use bevy::prelude::*;
use bevykit_core::schedule::KitSystems;

use bevy::input_focus::InputFocusVisible;
use crate::theme::UiTheme;

/// Interaction state of a widget. Maintained by the interaction and focus systems; read it to
/// style custom widgets.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct WidgetState {
    /// The widget ignores input and is drawn dimmed.
    pub disabled: bool,
    /// A pointer is over the widget.
    pub hovered: bool,
    /// The widget is being pressed.
    pub pressed: bool,
    /// The widget has input focus.
    pub focused: bool,
    /// The widget is selected or checked.
    pub selected: bool,
}

impl WidgetState {
    /// Returns `true` if the widget accepts input.
    pub fn is_interactive(&self) -> bool {
        !self.disabled
    }
}

/// How a widget is drawn. Widgets with this component are restyled from the theme whenever
/// their state or the theme changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[require(WidgetState, BackgroundColor, BorderColor)]
pub enum WidgetVisuals {
    /// An accent-colored control for the main action.
    Primary,
    /// A raised control with a border.
    #[default]
    Secondary,
    /// A control without background until hovered.
    Ghost,
    /// A panel background.
    Surface,
    /// The groove of a slider, toggle, or progress bar.
    Track,
    /// The filled part of a slider or progress bar.
    Fill,
    /// No themed background; only the focus outline is managed.
    Unstyled,
}

impl WidgetVisuals {
    fn base(&self, theme: &UiTheme, state: &WidgetState) -> (Color, Color) {
        let palette = &theme.palette;
        match self {
            WidgetVisuals::Primary => (palette.accent, palette.accent),
            WidgetVisuals::Secondary if state.selected => (palette.accent, palette.accent),
            WidgetVisuals::Secondary => (palette.surface_raised, palette.border),
            WidgetVisuals::Ghost if state.selected => {
                (palette.accent.with_alpha(0.25), Color::NONE)
            }
            WidgetVisuals::Ghost => (Color::NONE, Color::NONE),
            WidgetVisuals::Surface => (palette.surface, palette.border),
            WidgetVisuals::Track => (palette.surface_raised, palette.border),
            WidgetVisuals::Fill => (palette.accent, Color::NONE),
            WidgetVisuals::Unstyled => (Color::NONE, Color::NONE),
        }
    }

    /// Returns the background and border colors for a state.
    pub fn colors(&self, theme: &UiTheme, state: &WidgetState) -> (Color, Color) {
        let (mut background, mut border) = self.base(theme, state);
        let interactive = !matches!(
            self,
            WidgetVisuals::Surface | WidgetVisuals::Fill | WidgetVisuals::Unstyled
        );
        if interactive && !state.disabled {
            if state.pressed {
                background = shade(background, -0.12);
            } else if state.hovered {
                background = if *self == WidgetVisuals::Ghost && !state.selected {
                    theme.palette.text.with_alpha(0.08)
                } else {
                    shade(background, 0.08)
                };
            }
        }
        if state.disabled {
            background = background.with_alpha(background.alpha() * 0.4);
            border = border.with_alpha(border.alpha() * 0.4);
        }
        (background, border)
    }
}

/// Lightens (positive) or darkens (negative) a color, keeping its alpha.
fn shade(color: Color, amount: f32) -> Color {
    let mut oklch = Oklcha::from(color);
    oklch.lightness = (oklch.lightness + amount).clamp(0.0, 1.0);
    Color::from(oklch)
}

/// Restyles widgets whose state, visuals, or theme changed.
pub fn apply_widget_visuals(
    theme: Res<UiTheme>,
    focus_visibility: Res<InputFocusVisible>,
    mut commands: Commands,
    mut widgets: Query<(
        Entity,
        Ref<WidgetVisuals>,
        Ref<WidgetState>,
        &mut BackgroundColor,
        &mut BorderColor,
        Has<Outline>,
    )>,
) {
    let global_change = theme.is_changed() || focus_visibility.is_changed();
    for (entity, visuals, state, mut background, mut border, has_outline) in &mut widgets {
        if !global_change && !visuals.is_changed() && !state.is_changed() {
            continue;
        }
        let (background_color, border_color) = visuals.colors(&theme, &state);
        background.set_if_neq(BackgroundColor(background_color));
        border.set_if_neq(BorderColor::all(border_color));

        let show_outline = state.focused && focus_visibility.0;
        if show_outline {
            commands.entity(entity).insert(Outline::new(
                Val::Px(theme.focus_width),
                Val::Px(theme.focus_width),
                theme.palette.focus_ring,
            ));
        } else if has_outline {
            commands.entity(entity).remove::<Outline>();
        }
    }
}

pub(crate) fn build(app: &mut App) {
    app.register_type::<WidgetState>()
        .register_type::<WidgetVisuals>()
        .add_systems(
            PostUpdate,
            apply_widget_visuals.in_set(KitSystems::Bindings),
        );
}
