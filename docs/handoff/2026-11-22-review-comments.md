# Review comments (2026-11-22)

Reviewers leave comments on a video in the editor, anchored to what the
composition names rather than to source offsets; agents list, reply to, and
resolve them through the MCP server. Modelled on how Claude's Motion film
spec hangs comments and edits on a stable scene id.

## Specification

**Storage.** One JSON file per thread,
`<project root>/celesta-comments/<id>.json` (`celesta_project::review`,
`COMMENTS_DIR`). The root is `celesta_plugin_host::project_root` of the
entry. One file per thread so two reviewers adding comments never conflict
in git; replies to one thread can. Fields (`Comment`, camelCase):

| Field | Meaning |
| --- | --- |
| `version` | 1 (`FORMAT_VERSION`); a newer one is refused with "update Celesta" |
| `id` | the file stem: `c-` + 10 hex digits (`new_id`); lowercase, digits, `-` |
| `entry` | the entry, relative to the root with `/` (`CommentStore::entry_key`) |
| `composition` | only when the entry declares several roots |
| `anchor.chapter` | a chapter id (`<Composition chapters>`, `defineScenes`) |
| `anchor.from`, `anchor.to` | frames from the chapter's first frame (absolute without `chapter`); `to` exclusive, absent for one frame |
| `anchor.layer` | a layer id |
| `anchor.point` | `{x, y}` fractions of the canvas, 0..=1 |
| `lastSeen` | `{frame, chapterName}` where the anchor resolved when someone last acted on the thread |
| `author`, `createdAt`, `body` | `createdAt` RFC 3339 UTC to the second (`now_utc`, no date crate) |
| `resolved` | `{by, at}`; absent while open |
| `replies` | `[{id, author, createdAt, body}]` |

Unknown fields are kept on rewrite (`#[serde(flatten)] extra`). Writes go
to `.<id>.json.tmp` and are renamed over the file. A file that does not
parse, or whose `id` is not its file name, is a `LoadProblem`; the others
still load.

**Placement.** `Comment::place(chapters)` adds the chapter's `from` to the
anchor's frames. A chapter id that is no longer declared makes the comment
`Placement::Orphaned { last_seen }`: kept, listed, and seekable to
`lastSeen`, but without a timeline mark. `lastSeen` is written only when a
comment is created, replied to, or resolved in the editor (where the
chapters are at hand), never on open, so opening a project leaves git
clean. A comment without `composition` shows on the entry's first root only.

**Author.** `default_author(root)`: `CELESTA_AUTHOR`, `git -C root config
user.name`, `USER`/`USERNAME`, `Anonymous`. MCP replies default to `agent`.

**Editor** (`crates/editor/src/review.rs` model, `review_panel.rs` view):

- A Comments tab (`RightTab::Comments`) between Plugins and Problems in
  both modes, counting open comments on the shown composition.
- **Add comment…** (`C`, Mark → Add Comment…, `AddComment`) opens the
  composer on the playhead frame, or the In/Out span when both are set, in
  the chapter showing at its first frame (`anchor_at`), on the selected
  layer. **Pin** arms the canvas: the next press there sets the point
  (`press_pin`; layer picking is off while armed). `Enter` saves,
  `Shift Enter` is a newline, `Escape` cancels.
- Rows: number square, chapter name and timecode(s), layer, author, body
  (two lines), reply count, "Resolved", and for an orphan
  `Scene "…" no longer exists`. Clicking selects the comment, seeks to its
  frame, and selects its layer (React previews); the selected row opens its
  thread: replies, the resolution, a reply field, **Resolve** / **Reopen**,
  **Reply**. **Show resolved** lists resolved ones; numbers stay by creation
  order either way.
- Ruler marks (`comment_marks`): a 1rem numbered square at the first frame
  and a 2px line to the end of a span; open in `warning`, resolved muted,
  selected in the accent. A press selects it (it stops the scrub).
- Canvas pins (`comment_pins`): numbered `comment_pin_size()` squares
  centred on the point, for the selected comment and for open comments
  whose span covers the playhead (or whose frame it is), and a `+` pin for
  the draft.
- The folder is polled every 1.5 s (`watch_comments`, a `FolderStamp` of
  names, sizes, and mtimes) and reloaded when another program changed it.
  The text fields (`ReviewInputs`) are made with the window in `main.rs`
  and carried by `replace_contents`, with the selection, draft, and Show
  resolved when the same file reopens.

**MCP** (`crates/mcp/src/comments.rs`): `list_comments { entry,
composition?, status?: open|resolved|all, chapter?, check? }` loads the
entry through the bridge (`inspect::spawn_bridge`, factored out of
`inspect`) and returns each comment with `frame`/`toFrame`/`seconds`,
`chapterName`, and `orphaned: null | "chapter" | "layer"` (`layer` when no
layer with the id is in the scene at the comment's frame; at most 50
distinct frames are evaluated for that, `LAYER_CHECK_FRAMES`). A bridge that
fails to load is a warning and the comments are listed unplaced.
`reply_to_comment { entry, id, body, author? }` and `resolve_comment {
entry, id, reply?, author?, resolved? }` (false reopens). Errors:
`comment_not_found`, `invalid_comment`, `io`, `usage`. `inspect` and
`inspect.mjs` print `comments: N open (M orphaned), K resolved; read them
with list_comments` (and `comments` counts in JSON) when the composition has
any; `inspect.mjs` now also prints a `chapters:` line.

**Docs.** `skills/celesta/references/review.md` (also `read_docs` topic
`review`), a row and a workflow note in `SKILL.md`, a paragraph in the MCP
server's instructions, and "Leave review comments" plus the `C` shortcut in
the en/ja preview guide.

## Decisions

- The model lives in `celesta-project` (no GPUI), not a new crate: a new
  publishable crate fails CI's `Crate names` job until a crates.io
  placeholder exists.
- Frames, not seconds: chapters are in frames and agents pass frames to
  `render_frames`. A frame-rate change moves comments; that is rare.
- Points as fractions so a resolution change keeps them on the picture.
- Orphans are not re-anchored automatically (a renamed scene id could be
  guessed from `lastSeen.chapterName`, but a wrong guess moves a comment
  silently); the agent is told to say so in its reply.
- No `add_comment` MCP tool: the request was for agents to read and resolve.

## Checks

See the PR for the commands run and their results; screenshots of the UI
were taken by the user (the dev editor does not launch from an agent's
shell, see the editor launch note in memory).
