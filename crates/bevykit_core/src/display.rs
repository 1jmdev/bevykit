//! Display information shared between platform integration and UI layout.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Insets, in logical pixels, of the region of the screen that is not obscured by notches,
/// rounded corners, system bars, or other platform overlays.
///
/// Platform integration keeps this resource current; UI roots that respect the safe area
/// read it when laying out.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Debug, PartialEq, Default)]
pub struct SafeAreaInsets {
    /// Inset from the top edge.
    pub top: f32,
    /// Inset from the bottom edge.
    pub bottom: f32,
    /// Inset from the left edge.
    pub left: f32,
    /// Inset from the right edge.
    pub right: f32,
}

impl SafeAreaInsets {
    /// Insets of zero on every edge.
    pub const ZERO: Self = Self {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    };

    /// Creates insets with the same value on every edge.
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            bottom: value,
            left: value,
            right: value,
        }
    }

    /// Returns the safe rectangle within a viewport of the given logical size, with the origin
    /// at the top-left corner.
    pub fn safe_rect(&self, viewport: Vec2) -> Rect {
        let min = Vec2::new(self.left, self.top);
        let max = Vec2::new(viewport.x - self.right, viewport.y - self.bottom);
        Rect::from_corners(min, max.max(min))
    }
}
