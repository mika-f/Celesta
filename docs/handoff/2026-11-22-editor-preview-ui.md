# Editor readability and preview transport

Make Celesta's desktop editor easier to read during long sessions and make
playback the clearest action in the preview. This pass changes editor chrome,
not the rendered video or the composition API.

- Raise the shared editor rem from 13 to 14 px and brighten muted text while
  preserving the dark palette. Keep layout tests in step with the theme scale.
- Place mark buttons beside the timecode, give play/pause an outlined button,
  and group Loop playback with the viewer toggles. Keep the existing shortcuts
  and Mark menu as fallbacks when the viewer is narrow.
- Re-evaluate the toolbar layout when the measured viewer width crosses either
  rem breakpoint, so the controls follow panel resizes even while paused.

## Validation

- Editor and theme tests cover contrast, transport breakpoints, and inspector
  and timeline geometry at the editor rem.
- The PR shows the same frame of `examples/editor-demo.celesta.json` before and
  after, at 1440 × 900 on Linux/Wayland with Mesa software rendering.
- The preview guide in both languages and the Celesta Agent Skill describe
  where to find the mark controls and how to set a range at narrower widths.
