//! Modal layers: input blocking, stacking order, and backdrop dismissal.
//!
//! While a modal layer exists, it pushes its own input context that blocks the configured
//! contexts beneath it (gameplay by default), stacks above earlier layers, and, if its panel is
//! dismissible, closes when the player taps the backdrop outside the panel.

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;
use bevykit_input::prelude::*;

use crate::panel::Dismissible;

/// Stacking order of the first modal layer. Later layers stack above it.
pub const MODAL_BASE_Z_INDEX: i32 = 1000;

/// A full-screen layer hosting a modal panel.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct ModalLayer {
    /// The panel shown on the layer.
    pub panel: Entity,
    /// Input contexts blocked while the layer exists.
    pub blocks: Vec<InputContext>,
}

impl ModalLayer {
    /// Creates a layer for a panel that blocks gameplay input.
    pub fn new(panel: Entity) -> Self {
        Self {
            panel,
            blocks: vec![InputContext::Gameplay],
        }
    }

    fn context(entity: Entity) -> InputContext {
        InputContext::new(format!("modal-{}", entity.to_bits()))
    }
}

/// Modal layers in opening order.
#[derive(Resource, Default, Debug)]
pub struct ModalStack {
    layers: Vec<Entity>,
}

impl ModalStack {
    /// Returns the topmost layer.
    pub fn top(&self) -> Option<Entity> {
        self.layers.last().copied()
    }

    /// Returns `true` while any modal layer exists.
    pub fn is_active(&self) -> bool {
        !self.layers.is_empty()
    }
}

pub(crate) fn on_modal_added(
    added: On<Add, ModalLayer>,
    layers: Query<&ModalLayer>,
    mut stack: ResMut<ModalStack>,
    mut contexts: ResMut<InputContexts>,
    mut commands: Commands,
) {
    let Ok(layer) = layers.get(added.entity) else {
        return;
    };
    stack.layers.push(added.entity);
    let entry = contexts.push(ModalLayer::context(added.entity));
    for blocked in &layer.blocks {
        entry.block(blocked.clone());
    }
    let z_index = MODAL_BASE_Z_INDEX + stack.layers.len() as i32 * 10;
    commands.entity(added.entity).insert(GlobalZIndex(z_index));
}

pub(crate) fn on_modal_removed(
    removed: On<Remove, ModalLayer>,
    mut stack: ResMut<ModalStack>,
    mut contexts: ResMut<InputContexts>,
) {
    stack.layers.retain(|layer| *layer != removed.entity);
    contexts.remove(&ModalLayer::context(removed.entity));
}

/// Closes a dismissible modal when a press and release both land on its backdrop.
pub(crate) fn dismiss_on_backdrop(
    router: Res<PointerRouter>,
    stack: Res<ModalStack>,
    layers: Query<(&ModalLayer, Has<Dismissible>)>,
    mut commands: Commands,
) {
    let Some(top) = stack.top() else {
        return;
    };
    let Ok((layer, dismissible)) = layers.get(top) else {
        return;
    };
    if !dismissible {
        return;
    }
    let released_on_backdrop = router.pointers().any(|pointer| {
        pointer.phase() == PointerPhase::Released
            && pointer.owner().is_none()
            && pointer.press_targets().first() == Some(&top)
            && pointer.hover_targets().first() == Some(&top)
            && !pointer.is_over(layer.panel)
    });
    if released_on_backdrop {
        commands.entity(top).try_despawn();
    }
}
