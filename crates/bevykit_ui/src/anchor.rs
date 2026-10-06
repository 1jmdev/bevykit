//! UI positioned relative to entities or points in the world.
//!
//! ```ignore
//! ui.panel(PanelId::new("object-details"))
//!     .anchor(WorldAnchor::entity(target).camera(main_camera))
//!     .screen_offset(Vec2::new(0.0, -24.0))
//!     .clamp_to_safe_area()
//!     .build(build_object_details);
//! ```
//!
//! Anchors compute world positions with up-to-date transforms before layout, so anchored UI
//! does not lag a frame behind a moving target or camera.

use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;
use bevy::ui::ComputedNode;
use bevy::window::PrimaryWindow;
use bevykit_core::display::SafeAreaInsets;

/// What an anchor follows.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum AnchorTarget {
    /// An entity's position. When the entity is despawned the anchored node is hidden.
    Entity(Entity),
    /// A fixed world position.
    Point(Vec3),
}

/// What happens when the anchored position is outside the viewport or behind the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum OffscreenBehavior {
    /// Hide the node.
    #[default]
    Hide,
    /// Keep the node at the nearest viewport edge.
    ClampToEdge,
    /// Leave the node where the projection puts it.
    Keep,
}

/// Positions an absolutely positioned UI node at a world position seen through a camera.
#[derive(Component, Clone, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct WorldAnchor {
    /// The followed entity or point.
    pub target: AnchorTarget,
    /// The camera to project through. Without one, the first active camera is used.
    pub camera: Option<Entity>,
    /// Offset added to the world position, such as the height of a character's head.
    pub world_offset: Vec3,
    /// Offset added on screen, in logical pixels.
    pub screen_offset: Vec2,
    /// The point of the node placed at the anchor: `(0.5, 1.0)` is bottom center.
    pub pivot: Vec2,
    /// Keep the node inside the safe area.
    pub clamp_to_safe_area: bool,
    /// Behavior outside the viewport.
    pub offscreen: OffscreenBehavior,
}

impl WorldAnchor {
    /// Follows an entity.
    pub fn entity(target: Entity) -> Self {
        Self::new(AnchorTarget::Entity(target))
    }

    /// Stays at a world position.
    pub fn point(position: Vec3) -> Self {
        Self::new(AnchorTarget::Point(position))
    }

    fn new(target: AnchorTarget) -> Self {
        Self {
            target,
            camera: None,
            world_offset: Vec3::ZERO,
            screen_offset: Vec2::ZERO,
            pivot: Vec2::new(0.5, 1.0),
            clamp_to_safe_area: false,
            offscreen: OffscreenBehavior::Hide,
        }
    }

    /// Projects through the given camera.
    pub fn camera(mut self, camera: Entity) -> Self {
        self.camera = Some(camera);
        self
    }

    /// Adds a world-space offset.
    pub fn world_offset(mut self, offset: Vec3) -> Self {
        self.world_offset = offset;
        self
    }

    /// Adds a screen-space offset in logical pixels.
    pub fn screen_offset(mut self, offset: Vec2) -> Self {
        self.screen_offset = offset;
        self
    }

    /// Sets the pivot of the node placed at the anchor.
    pub fn pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// Sets the offscreen behavior.
    pub fn offscreen(mut self, behavior: OffscreenBehavior) -> Self {
        self.offscreen = behavior;
        self
    }

    /// Keeps the node inside the safe area.
    pub fn clamp_to_safe_area(mut self) -> Self {
        self.clamp_to_safe_area = true;
        self
    }
}

pub(crate) fn position_anchored_nodes(
    transforms: TransformHelper,
    cameras: Query<(Entity, &Camera)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    insets: Option<Res<SafeAreaInsets>>,
    mut anchored: Query<(&WorldAnchor, &mut Node, &mut Visibility, &ComputedNode)>,
) {
    let window_size = windows
        .single()
        .map(|window| Vec2::new(window.width(), window.height()))
        .unwrap_or(Vec2::ZERO);
    let insets = insets.map(|insets| *insets).unwrap_or_default();

    for (anchor, mut node, mut visibility, computed) in &mut anchored {
        let camera_entity = anchor.camera.or_else(|| {
            cameras
                .iter()
                .filter(|(_, camera)| camera.is_active)
                .max_by_key(|(_, camera)| camera.order)
                .map(|(entity, _)| entity)
        });
        let Some((camera_entity, camera)) = camera_entity.and_then(|entity| cameras.get(entity).ok())
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let world_position = match anchor.target {
            AnchorTarget::Point(point) => Some(point),
            AnchorTarget::Entity(entity) => transforms
                .compute_global_transform(entity)
                .ok()
                .map(|transform| transform.translation()),
        };
        let (Some(world_position), Ok(camera_transform)) = (
            world_position,
            transforms.compute_global_transform(camera_entity),
        ) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };

        let viewport = camera
            .logical_viewport_rect()
            .unwrap_or(Rect::from_corners(Vec2::ZERO, window_size));
        let projected = camera
            .world_to_viewport(&camera_transform, world_position + anchor.world_offset)
            .ok()
            .map(|position| position + viewport.min);
        let onscreen = projected.is_some_and(|position| viewport.contains(position));

        let position = match (projected, onscreen, anchor.offscreen) {
            (Some(position), true, _) | (Some(position), false, OffscreenBehavior::Keep) => position,
            (Some(position), false, OffscreenBehavior::ClampToEdge) => {
                position.clamp(viewport.min, viewport.max)
            }
            _ => {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
        };
        visibility.set_if_neq(Visibility::Inherited);

        let size = computed.size() * computed.inverse_scale_factor();
        let mut top_left = position + anchor.screen_offset - size * anchor.pivot;
        if anchor.clamp_to_safe_area && window_size != Vec2::ZERO {
            let safe = insets.safe_rect(window_size);
            let max = (safe.max - size).max(safe.min);
            top_left = top_left.clamp(safe.min, max);
        }

        let left = Val::Px(top_left.x.round());
        let top = Val::Px(top_left.y.round());
        if node.left != left || node.top != top || node.position_type != PositionType::Absolute {
            node.position_type = PositionType::Absolute;
            node.left = left;
            node.top = top;
        }
    }
}
