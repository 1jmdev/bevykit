//! Text labels.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::binding::{Binding, add_binding};
use crate::builder::{UiBuilder, WidgetBuilder};
use crate::text::{ThemedText, UiText};
use crate::theme::TextRole;

/// Builder for a label. Returned by [`UiBuilder::label`].
pub struct LabelBuilder<'a> {
    entity: EntityCommands<'a>,
}

impl<'a> WidgetBuilder<'a> for LabelBuilder<'a> {
    fn entity_commands(&mut self) -> &mut EntityCommands<'a> {
        &mut self.entity
    }
}

impl LabelBuilder<'_> {
    /// Sets the text role, selecting size and color.
    pub fn role(&mut self, role: TextRole) -> &mut Self {
        self.entity.insert(ThemedText::role(role));
        self
    }

    /// Shows text computed from resource `R`, updated whenever the resource changes.
    pub fn bind_resource<R: Resource, T: Into<UiText>>(
        &mut self,
        read: impl Fn(&R) -> T + Send + Sync + 'static,
    ) -> &mut Self {
        add_binding(
            &mut self.entity,
            Binding::resource(move |resource: &R| read(resource).into(), set_label_text),
        );
        self
    }
}

/// Replaces the text shown by a label entity.
pub fn set_label_text(entity: &mut EntityWorldMut, text: UiText) {
    match text {
        UiText::Plain(plain) => {
            #[cfg(feature = "localization")]
            entity.remove::<bevykit_data::localization::LocalizedText>();
            if let Some(mut current) = entity.get_mut::<Text>() {
                current.0 = plain.into_owned();
            }
        }
        #[cfg(feature = "localization")]
        UiText::Localized(localized) => {
            entity.insert(localized);
        }
    }
}

impl<'a, 'w, 's> UiBuilder<'a, 'w, 's> {
    /// Adds a text label.
    pub fn label(&mut self, text: impl Into<UiText>) -> LabelBuilder<'_> {
        let mut entity = self.spawn((ThemedText::body(), Pickable::IGNORE));
        text.into().insert(&mut entity);
        LabelBuilder { entity }
    }

    /// Adds a title label.
    pub fn heading(&mut self, text: impl Into<UiText>) -> LabelBuilder<'_> {
        let mut label = self.label(text);
        label.role(TextRole::Title);
        label
    }
}
