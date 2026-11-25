# External media drops in the editor (#582)

The paused standalone React viewer accepts a single image or video from
the OS, independently of Select layers. A file drag enables source
tracking when it was off. The topmost recorded hit is the only source
chain considered; an eligible enclosing Group can be used, but a branch
behind the hit, an unrelated selection, or an empty scene isn't a fallback.
The preview's drop message shows the parent and shared impact, or why
the drop is refused. Audio timeline placement, multiple files, and JSON
project drops are outside this stage.

The preview worker resolves the hit's JSX chain, timing, and parent
space. The editor uses Add's scope rules and geometry; it validates media
with the renderer or media backend before copying. The captured frame,
reload, JSX target and source text are checked before writing. The shared
Add write path records JSX and imports in one history entry. Files are
staged relative to the entry folder and published only while the drop is
still current; a published asset is not removed on undo or cancellation.

The drop handler validates the target again on release, retaining the
reason if it rejects the drop. GPUI's native macOS and Linux adapters do
not pass application-level validation back to the OS. The application
message, not the file manager's cursor, is the reliable acceptance
indicator. Fixing native feedback needs GPUI support.

Validation covers SVG probing, single-file acceptance and topmost-hit
targeting with editor tests, alongside the existing Add and history tests.
