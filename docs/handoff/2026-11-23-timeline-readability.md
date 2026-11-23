# Timeline readability

Continue the editor readability pass at the timeline without changing preview
size, frame rendering, scrubbing, or the track-header column width.

- The ruler uses the same surface as the preview transport. Larger timecodes
  read separately from the ticks and use the exact frame of each major tick.
  The label's shaped width determines whether it fits; a tick stays visible even
  when the label cannot fit. A width change requests a refresh while paused.
- Track names have more room between fixed controls. Hover over a shortened
  name to read it in full. Mute, Solo, and track state retain their slots.
- The geometry regression test covers a timecode that just fits, the narrower
  lane that can't show it, and the marks at the left and right ends.
