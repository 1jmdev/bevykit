//! Screen roots that respect the safe area, follow the language's writing direction, and
//! scale the interface.
//!
//! ```ignore
//! fn spawn_hud(mut ui: Ui) {
//!     ui.root()
//!         .respect_safe_area()
//!         .scale(UiScalePolicy::Logical)
//!         .build(|ui| {
//!             ui.label("Score: 0");
//!         });
//! }
//! ```

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::UiScale;
use bevy::window::PrimaryWindow;
use bevykit_core::display::SafeAreaInsets;

use crate::builder::{Ui, UiBuilder};

/// Pads a root node by the safe-area insets, so content avoids notches and system bars.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct SafeAreaRoot;

/// Sets a root node's writing direction from the selected language.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct FollowTextDirection;

/// How the interface scales with the window.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Resource)]
pub enum UiScalePolicy {
    /// One UI unit is one logical pixel. Leaves [`UiScale`] unchanged.
    #[default]
    Logical,
    /// Scales so that a layout designed for this logical size fits the window.
    Reference(Vec2),
    /// A fixed scale factor.
    Fixed(f32),
}

/// Builder for a screen root. Returned by [`Ui::root`].
pub struct RootBuilder<'u, 'w, 's> {
    ui: &'u mut Ui<'w, 's>,
    safe_area: bool,
    scale: Option<UiScalePolicy>,
}

impl RootBuilder<'_, '_, '_> {
    /// Keeps content inside the safe area.
    pub fn respect_safe_area(mut self) -> Self {
        self.safe_area = true;
        self
    }

    /// Sets the interface scale policy for the whole application.
    pub fn scale(mut self, policy: UiScalePolicy) -> Self {
        self.scale = Some(policy);
        self
    }

    /// Spawns the root and fills it with `build`. Returns the root entity.
    pub fn build(self, build: impl FnOnce(&mut UiBuilder)) -> Entity {
        if let Some(policy) = self.scale {
            self.ui.commands.insert_resource(policy);
        }
        let mut root = self.ui.commands.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            FollowTextDirection,
            Pickable::IGNORE,
        ));
        if self.safe_area {
            root.insert(SafeAreaRoot);
        }
        let root = root.id();
        let theme = self.ui.theme.clone();
        let mut builder = UiBuilder::new(&mut self.ui.commands, &theme, root);
        build(&mut builder);
        root
    }
}

impl<'w, 's> Ui<'w, 's> {
    /// Starts describing a full-screen root node.
    pub fn root(&mut self) -> RootBuilder<'_, 'w, 's> {
        RootBuilder {
            ui: self,
            safe_area: false,
            scale: None,
        }
    }
}

pub(crate) fn apply_safe_area(
    insets: Res<SafeAreaInsets>,
    mut roots: Query<(&mut Node, Ref<SafeAreaRoot>)>,
) {
    for (mut node, root) in &mut roots {
        if !insets.is_changed() && !root.is_added() {
            continue;
        }
        node.padding = UiRect {
            left: Val::Px(insets.left),
            right: Val::Px(insets.right),
            top: Val::Px(insets.top),
            bottom: Val::Px(insets.bottom),
        };
    }
}

pub(crate) fn apply_scale_policy(
    policy: Option<Res<UiScalePolicy>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut scale: ResMut<UiScale>,
) {
    let Some(policy) = policy else {
        return;
    };
    let target = match *policy {
        UiScalePolicy::Logical => return,
        UiScalePolicy::Fixed(factor) => factor,
        UiScalePolicy::Reference(reference) => {
            let Ok(window) = windows.single() else {
                return;
            };
            let size = Vec2::new(window.width(), window.height());
            if reference.x <= 0.0 || reference.y <= 0.0 {
                return;
            }
            (size / reference).min_element()
        }
    };
    if (scale.0 - target).abs() > f32::EPSILON {
        scale.0 = target;
    }
}

#[cfg(feature = "localization")]
pub(crate) fn apply_text_direction(
    locale: Option<Res<bevykit_data::localization::Locale>>,
    mut roots: Query<(&mut Node, Ref<FollowTextDirection>)>,
) {
    use bevy::ui::InlineDirection;
    use bevykit_data::localization::TextDirection;

    let Some(locale) = locale else {
        return;
    };
    let direction = match locale.direction() {
        TextDirection::Ltr => InlineDirection::Ltr,
        TextDirection::Rtl => InlineDirection::Rtl,
    };
    for (mut node, follow) in &mut roots {
        if (locale.is_changed() || follow.is_added()) && node.direction != direction {
            node.direction = direction;
        }
    }
}
