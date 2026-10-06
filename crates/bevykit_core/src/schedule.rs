//! Documented system ordering shared by every bevykit module.
//!
//! | Schedule     | Set                          | Purpose                                         |
//! | ------------ | ---------------------------- | ----------------------------------------------- |
//! | `PreUpdate`  | [`KitSystems::Platform`]     | Lifecycle, display, and clock sampling          |
//! | `PreUpdate`  | [`KitSystems::Input`]        | Action state, pointers, and gestures            |
//! | `PreUpdate`  | [`KitSystems::Interaction`]  | Focus, activation, and widget behavior          |
//! | `Update`     | (game systems)               | Gameplay reads the input produced above         |
//! | `PostUpdate` | [`KitSystems::Bindings`]     | Data bindings and localized text, before layout |
//! | `PostUpdate` | [`KitSystems::Presentation`] | Tweens, anchors, feedback, and audio            |
//! | `Last`       | [`KitSystems::Cleanup`]      | Expiry and end-of-frame bookkeeping             |
//!
//! The sets run in the listed order. Systems in [`KitSystems::Input`] run after Bevy's own
//! input processing and picking, so the data they read is current for the frame.

use bevy::input::InputSystems;
use bevy::transform::TransformSystems;
use bevy::prelude::*;

/// System sets used by bevykit modules. See the [module documentation](self) for the order.
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KitSystems {
    /// Platform sampling in `PreUpdate`.
    Platform,
    /// Input processing in `PreUpdate`.
    Input,
    /// Widget interaction in `PreUpdate`.
    Interaction,
    /// Data bindings in `PostUpdate`.
    Bindings,
    /// Presentation in `PostUpdate`.
    Presentation,
    /// Cleanup in `Last`.
    Cleanup,
}

pub(crate) fn configure_sets(app: &mut App) {
    app.configure_sets(
        PreUpdate,
        (
            KitSystems::Platform,
            KitSystems::Input,
            KitSystems::Interaction,
        )
            .chain()
            .after(InputSystems),
    )
    .configure_sets(
        PostUpdate,
        (KitSystems::Bindings, KitSystems::Presentation)
            .chain()
            .before(TransformSystems::Propagate),
    )
    .configure_sets(Last, KitSystems::Cleanup);
}
