# Editor readability and preview transport

Make Celesta's desktop editor easier to read during long sessions and make
playback the clearest action in the preview. This pass changes editor chrome,
not the rendered video or the composition API.

- Raise the shared editor rem from 13 to 14 px and brighten muted text while
  preserving the dark palette. Keep layout tests in step with the theme scale.
- Place mark buttons beside the timecode, give play/pause an outlined button,
  and group Loop playback with the viewer toggles. Keep the existing shortcuts
  and Mark menu as fallbacks when the viewer is narrow.
- Re-evaluate the toolbar layout across viewer widths even while paused. Keep
  the bar to one row so a narrow layout does not take height from the canvas.
  At the smallest widths, start/end jumps remain in the Playback menu while
  stepping, playback, and viewer toggles stay on the bar.

## Validation

- Editor and theme tests cover contrast, transport breakpoints, and inspector
  and timeline geometry at the editor rem.
- The PR shows the same frame of `examples/editor-demo.celesta.json` before and
  after, at 1440 × 900 on Linux/Wayland with Mesa software rendering.
- At the 260 px viewer minimum, Linux UI testing verified that the timecode,
  transport, and canvas remain visible with the timeline fully expanded.
  React Select layers was not tested.
- The preview guide in both languages and the Celesta Agent Skill describe
  where to find the mark controls and how to set a range at narrower widths.
