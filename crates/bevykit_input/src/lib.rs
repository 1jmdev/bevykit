#![doc = include_str!("../README.md")]

extern crate self as bevykit_input;

pub mod action;
pub mod binding;
pub mod context;
pub mod fixed;
pub mod gesture;
pub mod map;
pub mod pointer;
pub mod rebind;
mod update;
pub mod virtual_controls;

use std::marker::PhantomData;

use bevy::picking::PickingSystems;
use bevy::prelude::*;
use bevy::window::{AppLifecycle, WindowFocused};
use bevykit_core::schedule::KitSystems;

pub use bevykit_macros::KitAction;

/// Commonly used items.
pub mod prelude {
    pub use crate::action::{Action, ActionData, ActionState};
    pub use crate::binding::{
        Axis2Binding, AxisBinding, Binding, ButtonBinding, GamepadStick, KeyboardAxis,
        VirtualControl,
    };
    pub use crate::context::{InputContext, InputContexts};
    pub use crate::fixed::FixedActionState;
    pub use crate::gesture::{
        GestureCancelled, GestureDoubleTap, GestureDrag, GestureDragEnd, GestureDragStart,
        GestureHold, GesturePinch, GesturePolicy, GestureRotate, GestureTap, InteractionSurface,
    };
    pub use crate::map::{DeadZone, GamepadAssignment, InputMap, SavedBindings};
    pub use crate::pointer::{CaptureEnd, PointerCaptureLost, PointerPhase, PointerRouter};
    pub use crate::rebind::{RebindFinished, Rebinding};
    pub use crate::virtual_controls::{VirtualButton, VirtualControls, VirtualStick};
    pub use crate::{InputSuspension, KitAction, KitInputPlugin, KitInputSystems};
}

/// Ordering of input systems within [`KitSystems::Input`].
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KitInputSystems {
    /// Pointer sampling and capture arbitration.
    Pointers,
    /// Gesture recognition and virtual controls.
    Recognizers,
    /// Action state evaluation.
    Actions,
}

/// Whether input is suspended because the window lost focus or the app moved to the
/// background. While suspended, actions read as released and pointers are cancelled.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct InputSuspension {
    unfocused: bool,
    backgrounded: bool,
}

impl InputSuspension {
    /// Returns `true` while input is suspended.
    pub fn is_suspended(&self) -> bool {
        self.unfocused || self.backgrounded
    }
}

fn update_suspension(
    mut suspension: ResMut<InputSuspension>,
    mut focus: MessageReader<WindowFocused>,
    mut lifecycle: MessageReader<AppLifecycle>,
) {
    for event in focus.read() {
        suspension.unfocused = !event.focused;
    }
    for event in lifecycle.read() {
        suspension.backgrounded = matches!(
            event,
            AppLifecycle::WillSuspend | AppLifecycle::Suspended | AppLifecycle::Idle
        );
    }
}

/// Installs the action map, contexts, and fixed-tick queue for actions of type `A`, along with
/// the shared pointer router, gestures, and virtual controls.
///
/// Add one instance per action type.
pub struct KitInputPlugin<A: Action> {
    marker: PhantomData<A>,
}

impl<A: Action> Default for KitInputPlugin<A> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

use action::Action;

impl<A: Action> Plugin for KitInputPlugin<A> {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        if !app.is_plugin_added::<SharedInputPlugin>() {
            app.add_plugins(SharedInputPlugin);
        }

        app.init_resource::<action::ActionState<A>>()
            .init_resource::<map::InputMap<A>>()
            .init_resource::<fixed::FixedActionState<A>>()
            .init_resource::<rebind::Rebinding<A>>()
            .add_message::<rebind::RebindFinished<A>>()
            .add_systems(
                PreUpdate,
                (
                    rebind::capture_rebind::<A>,
                    update::update_action_state::<A>,
                    fixed::queue_fixed_edges::<A>,
                )
                    .chain()
                    .in_set(KitInputSystems::Actions),
            )
            .add_systems(FixedPreUpdate, fixed::deliver_fixed_edges::<A>);
    }
}

/// State shared by every action type. Added automatically by [`KitInputPlugin`].
struct SharedInputPlugin;

impl Plugin for SharedInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WindowFocused>()
            .add_message::<AppLifecycle>()
            .init_resource::<InputSuspension>()
            .init_resource::<context::InputContexts>()
            .init_resource::<pointer::PointerRouter>()
            .init_resource::<virtual_controls::VirtualControls>()
            .register_type::<gesture::InteractionSurface>()
            .register_type::<gesture::GesturePolicy>()
            .configure_sets(
                PreUpdate,
                (
                    KitInputSystems::Pointers,
                    KitInputSystems::Recognizers,
                    KitInputSystems::Actions,
                )
                    .chain()
                    .in_set(KitSystems::Input),
            )
            .configure_sets(PreUpdate, KitSystems::Input.after(PickingSystems::Hover))
            .add_systems(
                PreUpdate,
                (update_suspension, pointer::update_pointers)
                    .chain()
                    .in_set(KitInputSystems::Pointers),
            )
            .add_systems(
                PreUpdate,
                (
                    gesture::recognize_gestures,
                    virtual_controls::update_virtual_buttons,
                    virtual_controls::update_virtual_sticks,
                    pointer::flush_capture_events,
                )
                    .chain()
                    .in_set(KitInputSystems::Recognizers),
            );
    }
}
