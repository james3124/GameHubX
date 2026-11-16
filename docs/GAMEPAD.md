# Launcher gamepad mapping

The GameHubX launcher (not the Wine session) maps controller input to focus
navigation:

| Controller | Keyboard | Action |
|---|---|---|
| D-pad directions | arrows / Tab | Move focus |
| D-pad center | Enter | Activate focused control |
| A | Enter | Activate focused control |
| B | Escape / B | Back / close overlay |
| Y | Y | Search (dashboard) |
| Start | M | Open menu drawer |

Implementation:

- `GameHubXActivity.dispatchKeyEvent` remaps controller A/B/Y/Start to
  Enter/Escape/Y/M and leaves D-pad events to winit. This is best-effort because
  NativeActivity may route input on the native input queue.
- Winit maps Android D-pad to arrows and D-pad center to Enter.
- `focus.rs` records focusable controls; the app gives the first control focus
  when nothing is focused, so D-pad navigation works after the first frame.
- `widgets::btn`/`row` register focus targets and draw a visible focus ring;
  egui handles directional focus movement and Enter activation.
- Hints switch to controller glyphs on Android; desktop keeps keyboard hints.

Limitations:

- Button A/B/Y/Start depend on the Java remap reaching winit; D-pad and center
  are the guaranteed launcher path.
- Left/right stick axes, triggers and shoulder buttons are not mapped for
  launcher navigation.
- Gameplay controls are owned by Winlator/XInput inside the session and are not
  part of this mapping.
- No gamepad was available for an on-device verification yet.
