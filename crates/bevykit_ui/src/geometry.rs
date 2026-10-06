//! Conversions between UI layout data and logical screen coordinates.

use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};

/// Returns a node's rectangle in logical pixels, with the origin at the window's top-left
/// corner, matching pointer positions.
pub fn logical_rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    let scale = node.inverse_scale_factor();
    let center = transform.translation * scale;
    Rect::from_center_size(center, node.size() * scale)
}
