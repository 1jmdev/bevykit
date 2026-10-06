# bevykit_ui

Themeable widgets with complete interaction behavior, built on Bevy UI.

- **Widgets**: buttons, labels, toggles, sliders, progress bars, countdowns, text fields, tabs,
  tooltips, scroll views, and virtualized lists.
- **Interaction**: touch, mouse, keyboard, and controller share one model. Buttons activate on
  release and cancel when a press becomes a scroll.
- **Focus**: spatial navigation with explicit overrides; modal panels trap focus and restore
  it when they close.
- **Panels**: named screens composed once when opened, modal or not, with remembered scroll.
- **Bindings**: widgets follow resources without rebuilding.
- **World anchors**: UI that follows entities through a chosen camera.
- **Feedback**: floating text, notifications, camera shake, and haptics.
