//! Hover, press, and activation for widgets, shared by every input device.
//!
//! A [`Pressable`] widget activates when a pointer is pressed and released on it, or when it is
//! focused and the player confirms. Activation happens on release, never on press, and a press
//! is cancelled when another interaction (such as a scroll view) captures its pointer or the
//! pointer is released elsewhere. Activation triggers [`Activated`] on the widget and runs the
//! callbacks of its [`OnActivate`] component.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::input_focus::InputFocus;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevykit_input::prelude::*;

use crate::actions::UiAction;
use crate::state::WidgetState;

/// Makes a widget activatable by pointer release or by confirming while focused.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
#[require(WidgetState)]
pub struct Pressable {
    #[reflect(ignore)]
    pointer: Option<PointerId>,
}

/// Triggered on a widget when it is activated.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Activated {
    /// The activated widget.
    pub entity: Entity,
}

/// A callback run with commands and the widget entity.
pub type WidgetCallback = Arc<dyn Fn(&mut Commands, Entity) + Send + Sync>;

/// Callbacks run when the widget is activated.
#[derive(Component, Clone, Default)]
pub struct OnActivate(pub Vec<WidgetCallback>);

impl OnActivate {
    /// Adds a callback.
    pub fn push(&mut self, callback: impl Fn(&mut Commands, Entity) + Send + Sync + 'static) {
        self.0.push(Arc::new(callback));
    }
}

/// Triggered on a widget when the player changes its value.
#[derive(EntityEvent, Clone, Debug)]
pub struct ValueChanged<T: Clone + Send + Sync + 'static> {
    /// The widget.
    pub entity: Entity,
    /// The new value.
    pub value: T,
}

/// A callback run with commands and a new value.
pub type ValueCallback<T> = Arc<dyn Fn(&mut Commands, T) + Send + Sync>;

/// Callbacks run when the widget's value changes.
#[derive(Component, Clone)]
pub struct OnValueChange<T: Clone + Send + Sync + 'static>(pub Vec<ValueCallback<T>>);

impl<T: Clone + Send + Sync + 'static> Default for OnValueChange<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T: Clone + Send + Sync + 'static> OnValueChange<T> {
    /// Adds a callback.
    pub fn push(&mut self, callback: impl Fn(&mut Commands, T) + Send + Sync + 'static) {
        self.0.push(Arc::new(callback));
    }
}

/// Registers the observer that runs [`OnValueChange<T>`] callbacks.
pub(crate) struct ValueCallbacksPlugin<T>(PhantomData<T>);

impl<T> Default for ValueCallbacksPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Clone + Send + Sync + 'static> Plugin for ValueCallbacksPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_observer(
            |changed: On<ValueChanged<T>>,
             callbacks: Query<&OnValueChange<T>>,
             mut commands: Commands| {
                if let Ok(callbacks) = callbacks.get(changed.entity) {
                    for callback in &callbacks.0 {
                        callback(&mut commands, changed.value.clone());
                    }
                }
            },
        );
    }
}

pub(crate) fn run_activate_callbacks(
    activated: On<Activated>,
    callbacks: Query<&OnActivate>,
    mut commands: Commands,
) {
    if let Ok(callbacks) = callbacks.get(activated.entity) {
        for callback in &callbacks.0 {
            callback(&mut commands, activated.entity);
        }
    }
}

/// Updates [`WidgetState::hovered`] from the pointers.
pub(crate) fn update_hover(router: Res<PointerRouter>, mut widgets: Query<(Entity, &mut WidgetState)>) {
    for (entity, mut state) in &mut widgets {
        let hovered = router.pointers().any(|pointer| {
            pointer.is_over(entity)
                && (pointer.id() == PointerId::Mouse || pointer.is_down())
                && pointer.owner().is_none_or(|owner| owner == entity)
        });
        if state.hovered != hovered {
            state.hovered = hovered;
        }
    }
}

/// Tracks pointer presses and activates on release.
pub(crate) fn update_presses(
    router: Res<PointerRouter>,
    mut widgets: Query<(Entity, &mut Pressable, &mut WidgetState)>,
    mut commands: Commands,
) {
    for (entity, mut pressable, mut state) in &mut widgets {
        if state.disabled {
            pressable.pointer = None;
            if state.pressed {
                state.pressed = false;
            }
            continue;
        }

        if pressable.pointer.is_none()
            && let Some(press) = router.press_on(entity)
        {
            pressable.pointer = Some(press.id);
        }

        let Some(id) = pressable.pointer else {
            continue;
        };
        let pointer = router.get(id);
        let taken = pointer
            .and_then(|pointer| pointer.owner())
            .is_some_and(|owner| owner != entity);
        match pointer {
            Some(pointer) if !taken && pointer.is_down() => {
                let pressed = pointer.is_over(entity);
                if state.pressed != pressed {
                    state.pressed = pressed;
                }
            }
            Some(pointer)
                if !taken
                    && pointer.phase() == PointerPhase::Released
                    && pointer.is_over(entity) =>
            {
                pressable.pointer = None;
                state.pressed = false;
                commands.trigger(Activated { entity });
            }
            _ => {
                pressable.pointer = None;
                if state.pressed {
                    state.pressed = false;
                }
            }
        }
    }
}

/// Activates the focused widget when the player confirms.
pub(crate) fn activate_focused(
    mut actions: ResMut<ActionState<UiAction>>,
    focus: Option<Res<InputFocus>>,
    widgets: Query<&WidgetState, With<Pressable>>,
    mut commands: Commands,
) {
    if !actions.just_pressed(UiAction::Confirm) {
        return;
    }
    let Some(entity) = focus.and_then(|focus| focus.get()) else {
        return;
    };
    if widgets.get(entity).is_ok_and(WidgetState::is_interactive) {
        actions.consume(UiAction::Confirm);
        commands.trigger(Activated { entity });
    }
}
