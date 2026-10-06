# bevykit_input

Named input actions for Bevy, with contexts, rebinding, and a unified pointer model.

- **Actions**: game code reads `ActionState<GameAction>` instead of individual keys, buttons,
  or sticks. Every action has a button state, a scalar value, and a 2D axis.
- **Contexts**: bindings belong to contexts (gameplay, menus, dialogs) that can block one
  another.
- **Fixed ticks**: `FixedActionState` delivers each press to exactly one simulation tick.
- **Rebinding**: capture the next input, report conflicts, and save bindings with serde.
- **Pointers**: mouse and touch share one API, with single-owner capture.
- **Gestures**: taps, double taps, holds, drags, pinches, and rotations.
- **Virtual controls**: on-screen buttons and sticks that drive the same actions.

```rust,ignore
#[derive(KitAction, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum GameAction {
    Move,
    Interact,
    Pause,
}

fn configure_input(mut input: ResMut<InputMap<GameAction>>) {
    input
        .context(InputContext::Gameplay)
        .axis2(GameAction::Move, KeyboardAxis::wasd())
        .axis2(GameAction::Move, GamepadStick::Left)
        .button(GameAction::Interact, KeyCode::KeyE)
        .button(GameAction::Interact, GamepadButton::South)
        .button(GameAction::Pause, KeyCode::Escape);
}
```
