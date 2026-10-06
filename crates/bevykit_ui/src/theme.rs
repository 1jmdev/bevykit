//! Themes: colors, typography, spacing, and shape.
//!
//! ```ignore
//! fn configure_theme(mut theme: ResMut<UiTheme>, assets: Res<GameAssets>) {
//!     *theme = UiTheme::new()
//!         .font(assets.ui_font.clone())
//!         .spacing(8.0)
//!         .text_color(Color::WHITE)
//!         .accent_color(Color::srgb(0.25, 0.65, 0.9));
//! }
//! ```
//!
//! Widgets restyle themselves when the theme changes.

use bevy::prelude::*;

/// The colors used by widgets.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct Palette {
    /// Primary text.
    pub text: Color,
    /// Secondary text, such as hints and placeholders.
    pub text_muted: Color,
    /// Accent for primary actions, selection, and progress.
    pub accent: Color,
    /// Text drawn on the accent color.
    pub accent_text: Color,
    /// Panel backgrounds.
    pub surface: Color,
    /// Backgrounds of controls drawn on a surface.
    pub surface_raised: Color,
    /// Borders and separators.
    pub border: Color,
    /// The dimming layer behind modal panels.
    pub backdrop: Color,
    /// Destructive actions and errors.
    pub danger: Color,
    /// The outline drawn around the focused widget.
    pub focus_ring: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            text: Color::srgb(0.93, 0.94, 0.96),
            text_muted: Color::srgb(0.62, 0.65, 0.70),
            accent: Color::srgb(0.25, 0.55, 0.95),
            accent_text: Color::WHITE,
            surface: Color::srgb(0.11, 0.12, 0.14),
            surface_raised: Color::srgb(0.18, 0.19, 0.22),
            border: Color::srgb(0.28, 0.30, 0.34),
            backdrop: Color::srgba(0.0, 0.0, 0.0, 0.55),
            danger: Color::srgb(0.88, 0.30, 0.30),
            focus_ring: Color::srgb(0.98, 0.80, 0.30),
        }
    }
}

/// Fonts and text sizes used by widgets. Fonts are supplied by the game.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct Typography {
    /// The font for all widget text, or `None` for Bevy's default font.
    pub font: Option<Handle<Font>>,
    /// Size of body text.
    pub body_size: f32,
    /// Size of panel titles.
    pub title_size: f32,
    /// Size of captions, hints, and tooltips.
    pub small_size: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font: None,
            body_size: 18.0,
            title_size: 26.0,
            small_size: 14.0,
        }
    }
}

/// The role of a piece of text, selecting its size and color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum TextRole {
    /// Body text.
    #[default]
    Body,
    /// A panel title.
    Title,
    /// Small, muted text.
    Caption,
}

impl Typography {
    /// Returns the size for a text role.
    pub fn size(&self, role: TextRole) -> f32 {
        match role {
            TextRole::Body => self.body_size,
            TextRole::Title => self.title_size,
            TextRole::Caption => self.small_size,
        }
    }

    /// Returns the [`TextFont`] for a text role.
    pub fn text_font(&self, role: TextRole) -> TextFont {
        let font = TextFont::from_font_size(self.size(role));
        match &self.font {
            Some(handle) => font.with_font(handle.clone()),
            None => font,
        }
    }
}

/// The active theme.
#[derive(Resource, Clone, Debug, PartialEq, Reflect)]
#[reflect(Resource)]
pub struct UiTheme {
    /// Colors.
    pub palette: Palette,
    /// Fonts and sizes.
    pub typography: Typography,
    /// The base spacing unit in logical pixels. Padding and gaps are multiples of it.
    pub spacing: f32,
    /// Corner radius of controls and panels.
    pub radius: f32,
    /// Border width of controls.
    pub border_width: f32,
    /// Width of the focus outline.
    pub focus_width: f32,
    /// Minimum height of interactive controls, sized for touch.
    pub control_height: f32,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            typography: Typography::default(),
            spacing: 8.0,
            radius: 6.0,
            border_width: 1.0,
            focus_width: 2.0,
            control_height: 44.0,
        }
    }
}

impl UiTheme {
    /// Creates the default theme.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the font for all widget text.
    pub fn font(mut self, font: Handle<Font>) -> Self {
        self.typography.font = Some(font);
        self
    }

    /// Sets the base spacing unit.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Sets the corner radius.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Sets the primary text color.
    pub fn text_color(mut self, color: Color) -> Self {
        self.palette.text = color;
        self
    }

    /// Sets the accent color.
    pub fn accent_color(mut self, color: Color) -> Self {
        self.palette.accent = color;
        self
    }

    /// Sets the panel background color.
    pub fn surface_color(mut self, color: Color) -> Self {
        self.palette.surface = color;
        self
    }

    /// Replaces the palette.
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Replaces the typography.
    pub fn typography(mut self, typography: Typography) -> Self {
        self.typography = typography;
        self
    }

    /// Returns `multiplier` spacing units as a [`Val`].
    pub fn gap(&self, multiplier: f32) -> Val {
        Val::Px(self.spacing * multiplier)
    }
}
