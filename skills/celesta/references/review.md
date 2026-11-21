# Review comments

People review a video in the Celesta app and leave comments on it: "the
title comes in too late here", pinned to a scene, a moment or a span, and
optionally a layer and a spot on the canvas. Your job is to find each open
comment, change the source so it is addressed, check the frame it names,
and reply or resolve it.

## Where comments live

One JSON file per thread in the project root's `celesta-comments/` folder
(the root is the nearest folder with `celesta.config.json` above the entry,
else the entry's folder). They are meant to be committed with the source.

```json
{
  "version": 1,
  "id": "c-7f3a9b2e01",
  "entry": "film.tsx",
  "composition": "main",
  "anchor": {
    "chapter": "intro",
    "from": 45,
    "to": 90,
    "layer": "title",
    "point": { "x": 0.42, "y": 0.31 }
  },
  "lastSeen": { "frame": 165, "chapterName": "Intro" },
  "author": "Kanon",
  "createdAt": "2026-11-22T10:00:00Z",
  "body": "Make the title fade in faster",
  "resolved": { "by": "claude", "at": "2026-11-22T11:00:00Z" },
  "replies": [
    { "id": "r-1a2b3c4d5e", "author": "claude", "createdAt": "2026-11-22T11:00:00Z", "body": "Fade is 10 frames now" }
  ]
}
```

- `entry`: the entry the comment is on, relative to the project root.
  `composition` is there only when the entry declares several roots.
- `anchor.chapter`: a chapter id from `<Composition chapters>` (a
  `defineScenes` id). `from` and `to` count frames **from that chapter's
  first frame**, so the comment stays on its scene when earlier scenes get
  longer. Without `chapter` they are composition frames. `to` is the frame
  after the last of a span; without it the comment is on one frame.
- `anchor.layer`: a layer id (an explicit `id` prop, or the id the tree path
  gives). `anchor.point`: where on the canvas, as fractions of its width and
  height (0,0 is the top-left corner).
- `resolved` is absent while the comment is open.

Never edit these files by hand to resolve a comment; use the tools below,
which keep the file's other fields.

## Find open comments

With the MCP server, call `list_comments { entry }`. It loads the entry and
places each comment in the composition as it is now:

- `frame` (and `toFrame` for a span) and `seconds`: composition frames to
  pass to `render_frames` or `inspect`.
- `chapterName`: the scene's label.
- `orphaned`: `"chapter"` when the chapter id is no longer declared (the
  scene was removed or renamed; `lastSeenFrame` is where the comment was
  when someone last acted on it), `"layer"` when no layer with that id is
  drawn at that frame. Otherwise `null`.

`status: "all"` includes resolved comments; `chapter` lists one scene's.
`check: false` only reads the files (no frames), for when the entry does not
load.

Without the MCP server, `inspect.mjs` prints a `comments:` line when there
are any (`comments: 2 open (1 orphaned), 1 resolved`), and the files can be
read directly: add the chapter's `from` (from the `chapters:` line of
`inspect.mjs`) to `anchor.from`.

## Address a comment

1. Render the frame the comment names (`render_frames` with `frame`) and
   look at it, together with `layer` and `point`, to see what the reviewer
   saw.
2. Change the source. A comment on a span (`frame` to `toFrame`) is usually
   about timing: check a few frames inside it.
3. Render the frame again and look at it.
4. Resolve it with a short note of what changed:
   `resolve_comment { entry, id, reply: "Fade is 10 frames now", author: "<your name>" }`.
   If the comment is unclear or you disagree, `reply_to_comment` instead and
   leave it open; the reviewer sees the reply in the app.

For an orphaned comment, find the scene it was on from `chapterName` and
`lastSeenFrame` (a renamed `defineScenes` id is the usual cause). Address it
there, and say in the reply that its scene was renamed; the comment's anchor
is not moved.

`resolve_comment { resolved: false }` reopens a comment. Pass your name as
`author`; without it replies and resolutions are by `agent`.
