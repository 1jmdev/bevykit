//! Short-lived feedback: floating text, notifications, camera impulses, and haptics.
//!
//! ```ignore
//! fn reward(mut feedback: Feedback, target: Single<Entity, With<Chest>>) {
//!     feedback
//!         .floating_text("+10")
//!         .at_entity(*target)
//!         .style(FeedbackStyle::Reward)
//!         .duration(0.8);
//!     feedback.notify(tr!("save.complete"));
//!     feedback.haptic(HapticPattern::Success);
//! }
//! ```
//!
//! Feedback cleans itself up, and respects [`FeedbackSettings`] for reduced motion and haptics.

use bevy::ecs::system::SystemParam;
use bevy::picking::Pickable;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::GlobalZIndex;
use bevykit_core::platform::{HapticPattern, Haptics};

use crate::anchor::{OffscreenBehavior, WorldAnchor};
use crate::camera_shake::CameraShake;
use crate::lifetime::Lifetime;
use crate::notification::NotificationQueue;
use crate::text::UiText;
use crate::theme::UiTheme;

/// Player preferences that feedback respects.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource)]
pub struct FeedbackSettings {
    /// Replace motion (rising text, camera shake) with fades or nothing.
    pub reduce_motion: bool,
    /// Play haptic patterns.
    pub haptics: bool,
}

impl Default for FeedbackSettings {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            haptics: true,
        }
    }
}

/// Names a visual style for feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum FeedbackStyle {
    /// Neutral information.
    Info,
    /// Gains and successes.
    Reward,
    /// Losses and damage.
    Damage,
    /// Warnings.
    Warning,
    /// A game-defined style.
    Custom(u32),
}

/// How a feedback style looks. Fonts come from the theme unless set here.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct FeedbackLook {
    /// Text color.
    pub color: Color,
    /// Font size.
    pub size: f32,
    /// Font override.
    pub font: Option<Handle<Font>>,
}

/// The looks of feedback styles. Games override entries to match their art.
#[derive(Resource, Clone, Debug)]
pub struct FeedbackStyles {
    looks: HashMap<FeedbackStyle, FeedbackLook>,
}

impl Default for FeedbackStyles {
    fn default() -> Self {
        let look = |color: Color| FeedbackLook {
            color,
            size: 24.0,
            font: None,
        };
        let mut looks = HashMap::default();
        looks.insert(FeedbackStyle::Info, look(Color::WHITE));
        looks.insert(FeedbackStyle::Reward, look(Color::srgb(1.0, 0.85, 0.3)));
        looks.insert(FeedbackStyle::Damage, look(Color::srgb(1.0, 0.35, 0.3)));
        looks.insert(FeedbackStyle::Warning, look(Color::srgb(1.0, 0.65, 0.2)));
        Self { looks }
    }
}

impl FeedbackStyles {
    /// Sets the look of a style.
    pub fn set(&mut self, style: FeedbackStyle, look: FeedbackLook) -> &mut Self {
        self.looks.insert(style, look);
        self
    }

    /// Returns the look of a style, falling back to [`FeedbackStyle::Info`].
    pub fn get(&self, style: FeedbackStyle) -> FeedbackLook {
        self.looks
            .get(&style)
            .or_else(|| self.looks.get(&FeedbackStyle::Info))
            .cloned()
            .unwrap_or(FeedbackLook {
                color: Color::WHITE,
                size: 24.0,
                font: None,
            })
    }
}

/// Animates a floating text: rises and fades out over its lifetime.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct FloatingText {
    duration: f32,
    elapsed: f32,
    rise: f32,
    base_offset: Vec2,
}

/// System parameter for requesting feedback.
#[derive(SystemParam)]
pub struct Feedback<'w, 's> {
    commands: Commands<'w, 's>,
    settings: Res<'w, FeedbackSettings>,
    styles: Res<'w, FeedbackStyles>,
    theme: Res<'w, UiTheme>,
    haptics: Option<Res<'w, Haptics>>,
    notifications: ResMut<'w, NotificationQueue>,
    shakes: Query<'w, 's, &'static mut CameraShake>,
}

impl<'w, 's> Feedback<'w, 's> {
    /// Shows text that rises and fades.
    pub fn floating_text(&mut self, text: impl Into<UiText>) -> FloatingTextBuilder<'_, 'w, 's> {
        FloatingTextBuilder {
            feedback: self,
            text: Some(text.into()),
            position: FloatingPosition::Screen(Vec2::ZERO),
            style: FeedbackStyle::Info,
            duration: 0.8,
            rise: 48.0,
        }
    }

    /// Queues a notification.
    pub fn notify(&mut self, text: impl Into<UiText>) {
        self.notifications.push(text.into());
    }

    /// Plays a haptic pattern if haptics are enabled.
    pub fn haptic(&mut self, pattern: HapticPattern) {
        if self.settings.haptics
            && let Some(haptics) = &self.haptics
        {
            haptics.play(pattern);
        }
    }

    /// Shakes a camera. Adds a [`CameraShake`] to it if needed. Ignored with reduced motion.
    pub fn shake(&mut self, camera: Entity, trauma: f32) {
        if self.settings.reduce_motion {
            return;
        }
        match self.shakes.get_mut(camera) {
            Ok(mut shake) => shake.add_trauma(trauma),
            Err(_) => {
                let mut shake = CameraShake::new();
                shake.add_trauma(trauma);
                if let Ok(mut entity) = self.commands.get_entity(camera) {
                    entity.insert(shake);
                }
            }
        }
    }

    /// Spawns a temporary entity that despawns after `seconds`.
    pub fn spawn_temporary(&mut self, bundle: impl Bundle, seconds: f32) -> Entity {
        self.commands.spawn((bundle, Lifetime::seconds(seconds))).id()
    }
}

#[derive(Clone, Copy, Debug)]
enum FloatingPosition {
    Screen(Vec2),
    World(WorldAnchor),
}

/// Configures a floating text. It appears when the builder is dropped.
pub struct FloatingTextBuilder<'f, 'w, 's> {
    feedback: &'f mut Feedback<'w, 's>,
    text: Option<UiText>,
    position: FloatingPosition,
    style: FeedbackStyle,
    duration: f32,
    rise: f32,
}

impl FloatingTextBuilder<'_, '_, '_> {
    /// Places the text above an entity, projected through the active camera.
    pub fn at_entity(&mut self, entity: Entity) -> &mut Self {
        self.position = FloatingPosition::World(
            WorldAnchor::entity(entity)
                .pivot(Vec2::new(0.5, 1.0))
                .offscreen(OffscreenBehavior::Hide),
        );
        self
    }

    /// Places the text at a world position.
    pub fn at_point(&mut self, point: Vec3) -> &mut Self {
        self.position = FloatingPosition::World(WorldAnchor::point(point));
        self
    }

    /// Places the text at a screen position in logical pixels.
    pub fn at_screen(&mut self, position: Vec2) -> &mut Self {
        self.position = FloatingPosition::Screen(position);
        self
    }

    /// Projects through a specific camera.
    pub fn camera(&mut self, camera: Entity) -> &mut Self {
        if let FloatingPosition::World(anchor) = &mut self.position {
            anchor.camera = Some(camera);
        }
        self
    }

    /// Sets the style.
    pub fn style(&mut self, style: FeedbackStyle) -> &mut Self {
        self.style = style;
        self
    }

    /// Sets how long the text stays, in seconds.
    pub fn duration(&mut self, seconds: f32) -> &mut Self {
        self.duration = seconds.max(0.05);
        self
    }

    /// Sets how far the text rises, in logical pixels.
    pub fn rise(&mut self, distance: f32) -> &mut Self {
        self.rise = distance;
        self
    }
}

impl Drop for FloatingTextBuilder<'_, '_, '_> {
    fn drop(&mut self) {
        let Some(text) = self.text.take() else {
            return;
        };
        let look = self.feedback.styles.get(self.style);
        let font = look
            .font
            .clone()
            .or_else(|| self.feedback.theme.typography.font.clone());
        let mut text_font = TextFont::from_font_size(look.size);
        if let Some(font) = font {
            text_font = text_font.with_font(font);
        }
        let rise = if self.feedback.settings.reduce_motion {
            0.0
        } else {
            self.rise
        };
        let (node, anchor, base_offset) = match self.position {
            FloatingPosition::Screen(position) => (
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(position.x),
                    top: Val::Px(position.y),
                    ..default()
                },
                None,
                Vec2::ZERO,
            ),
            FloatingPosition::World(anchor) => (
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Some(anchor),
                anchor.screen_offset,
            ),
        };
        let mut entity = self.feedback.commands.spawn((
            node,
            text_font,
            TextColor(look.color),
            GlobalZIndex(i32::MAX - 10),
            Pickable::IGNORE,
            FloatingText {
                duration: self.duration,
                elapsed: 0.0,
                rise,
                base_offset,
            },
            Lifetime::seconds(self.duration),
        ));
        text.insert(&mut entity);
        if let Some(anchor) = anchor {
            entity.insert(anchor);
        }
    }
}

pub(crate) fn animate_floating_text(
    time: Res<Time<Real>>,
    mut texts: Query<(
        &mut FloatingText,
        &mut TextColor,
        &mut Node,
        Option<&mut WorldAnchor>,
    )>,
) {
    for (mut floating, mut color, mut node, anchor) in &mut texts {
        floating.elapsed += time.delta_secs();
        let progress = (floating.elapsed / floating.duration).clamp(0.0, 1.0);
        let eased_rise = floating.rise * (1.0 - (1.0 - progress).powi(3));
        let alpha = if progress < 0.6 {
            1.0
        } else {
            1.0 - (progress - 0.6) / 0.4
        };
        color.0.set_alpha(alpha);
        match anchor {
            Some(mut anchor) => {
                anchor.screen_offset = floating.base_offset - Vec2::Y * eased_rise;
            }
            None => {
                node.margin.top = Val::Px(-eased_rise);
            }
        }
    }
}
