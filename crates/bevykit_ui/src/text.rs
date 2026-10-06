//! Widget text: plain or localized content, styled by the theme.

use std::borrow::Cow;

use bevy::prelude::*;
use bevykit_core::schedule::KitSystems;
#[cfg(feature = "localization")]
use bevykit_data::localization::{Locale, LocalizedText};

use crate::theme::{TextRole, UiTheme};

/// Text for a widget: either fixed text or a translation request.
#[derive(Clone, Debug, PartialEq)]
pub enum UiText {
    /// Text shown as written.
    Plain(Cow<'static, str>),
    /// A translation that follows the selected language.
    #[cfg(feature = "localization")]
    Localized(LocalizedText),
}

impl UiText {
    /// Returns the text to show before translation runs: the plain text or the message key.
    pub fn initial(&self) -> String {
        match self {
            UiText::Plain(text) => text.to_string(),
            #[cfg(feature = "localization")]
            UiText::Localized(localized) => localized.key.to_string(),
        }
    }

    /// Inserts the components that display this text on an entity.
    pub(crate) fn insert(self, entity: &mut EntityCommands) {
        entity.insert(Text::new(self.initial()));
        #[cfg(feature = "localization")]
        if let UiText::Localized(localized) = self {
            entity.insert(localized);
        }
    }
}

impl From<&'static str> for UiText {
    fn from(text: &'static str) -> Self {
        Self::Plain(Cow::Borrowed(text))
    }
}

impl From<String> for UiText {
    fn from(text: String) -> Self {
        Self::Plain(Cow::Owned(text))
    }
}

impl From<Cow<'static, str>> for UiText {
    fn from(text: Cow<'static, str>) -> Self {
        Self::Plain(text)
    }
}

#[cfg(feature = "localization")]
impl From<LocalizedText> for UiText {
    fn from(text: LocalizedText) -> Self {
        Self::Localized(text)
    }
}

/// Styles a text entity from the theme. Restyled whenever the theme changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct ThemedText {
    /// The role selecting size and color.
    pub role: TextRole,
    /// Use the accent text color, for text drawn on an accent background.
    pub on_accent: bool,
}

impl ThemedText {
    /// Body text.
    pub fn body() -> Self {
        Self::default()
    }

    /// Text with the given role.
    pub fn role(role: TextRole) -> Self {
        Self {
            role,
            on_accent: false,
        }
    }

    /// Returns the font and color for this text.
    pub fn resolve(&self, theme: &UiTheme) -> (TextFont, TextColor) {
        let color = if self.on_accent {
            theme.palette.accent_text
        } else if self.role == TextRole::Caption {
            theme.palette.text_muted
        } else {
            theme.palette.text
        };
        (theme.typography.text_font(self.role), TextColor(color))
    }
}

pub(crate) fn apply_themed_text(
    theme: Res<UiTheme>,
    mut texts: Query<(Ref<ThemedText>, &mut TextFont, &mut TextColor)>,
) {
    let theme_changed = theme.is_changed();
    for (themed, mut font, mut color) in &mut texts {
        if !theme_changed && !themed.is_changed() {
            continue;
        }
        let (new_font, new_color) = themed.resolve(&theme);
        font.set_if_neq(new_font);
        color.set_if_neq(new_color);
    }
}

/// Translates every [`LocalizedText`] whose request changed, and all of them when the language
/// or a translation file changes.
#[cfg(feature = "localization")]
pub(crate) fn apply_localized_text(
    locale: Option<Res<Locale>>,
    mut texts: Query<(Ref<LocalizedText>, &mut Text)>,
    mut last_revision: Local<Option<u64>>,
) {
    let Some(locale) = locale else {
        return;
    };
    let revision_changed = *last_revision != Some(locale.revision());
    *last_revision = Some(locale.revision());
    for (localized, mut text) in &mut texts {
        if !revision_changed && !localized.is_changed() {
            continue;
        }
        let translated = locale.translate(&localized);
        if text.0 != translated {
            text.0 = translated;
        }
    }
}

pub(crate) fn build(app: &mut App) {
    app.register_type::<ThemedText>()
        .add_systems(PostUpdate, apply_themed_text.in_set(KitSystems::Bindings));
    #[cfg(feature = "localization")]
    app.add_systems(
        PostUpdate,
        apply_localized_text.in_set(KitSystems::Bindings),
    );
}
