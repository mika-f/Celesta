# The VRM plugin's texture cache is swept (2026-11-08)

The VRM plugin writes each image a model's materials use to its texture
cache as `<sha256>.<png|jpg|webp>` and sends Celesta the path (see
`2026-11-03-plugin-vrm.md`, "Materials and textures"). Until now nothing
removed those files, so the cache grew with every avatar and every edit of
one. `plugins/vrm/src/texture_cache.rs` now sweeps it at each model open.

## What Celesta needs from the files

- `celesta-gpu-renderer` decodes every `TextureSource::Path` of a model in
  the same call that opened it (`mesh.rs`, the `provider.open` branch), and
  never reads the files again. A model the renderer drops is opened again
  through the plugin, which writes or finds its images again. A model that
  failed to load keeps the files' stamps to retry when they change: a
  removed file is a change, and the retry opens the model again.
- So a file must survive only from the open that wrote or found it until
  the renderer has read it, in whichever process (editor preview, a second
  window, the exporter, each with its own plugin process and one shared
  cache).

## Specification

- **Last use is the file's modification time.** A new image is a new file
  (temporary file renamed into place). An open that finds its image already
  there opens it (not through a link: `O_NOFOLLOW` on Unix, after
  `symlink_metadata` says it is a file), sets its time to now, then reads
  and compares the bytes through the same handle. A file whose time cannot
  be set is written again; when that write fails too (a cache that cannot
  be written, which no sweep can empty either), the file is read without a
  write handle and used as it is when it holds the bytes, as before.
- **The lock** is `celesta-plugin-vrm.lock` in the cache (0600 on Unix,
  never through a link), with `std::fs::File` locks (`flock` on Unix,
  `LockFileEx` on Windows). `materials` holds it shared while it writes and
  finds the model's images; the sweep takes it exclusive with `try_lock`
  and is skipped when it cannot. Opens wait (briefly) for a sweep to end;
  opens in many processes still write side by side. Where the file cannot
  be made or locked, opens write without it and no sweep can run.
- **The sweep** runs after every open that wrote or found an image, once
  the shared lock is released. An open without images neither makes nor
  sweeps the cache: the cache grows only through opens that write, and
  each of those sweeps. Only regular files the plugin names are
  considered: `<64 lowercase hex>.<png|jpg|webp>` and the temporary files
  `<that>.<pid>-<n>.tmp`. Other files, directories, and links stay, so the
  cache may be a folder of the user's own (`CELESTA_VRM_TEXTURE_CACHE`).
  - Nothing used within `IN_USE` (1 day) is removed, whatever the size.
  - A temporary file older than `IN_USE` is one a process left when it
    died, and is removed.
  - Images are taken least recently used first: removed while unused for
    more than `MAX_AGE` (30 days), or while all the images take more than
    `SIZE_BUDGET` (2 GiB).
  - A time ahead of the clock counts as just used.
  - Errors never fail the open: a file the sweep cannot read or remove
    stays.
- **Why the lock and the time together.** Without the lock, a sweep could
  read a file's old time, an open find and mark that file and answer its
  path, and the sweep then remove it before the renderer reads it. With
  the lock, a sweep sees either the old time before the open looks (and the
  open then finds no file and writes it again) or the new time after. The
  grace period covers the moment between the open's answer and the
  renderer's read.
- The existing checks on the cache folder (absolute, not a link, a
  directory, the user's own, not writable by others) are unchanged and run
  before the lock file is made.

## Docs

- The user guide's VRM cache paragraph
  (`packages/website/content/{en,ja}/docs/05-(more)/05-plugins.md`) says
  when images and stale temporary files are removed, that recently used
  ones stay, that other files stay, and how to empty the cache by hand
  (the default folder whole; a folder of the user's own, only the
  plugin's names).
- `skills/celesta/references/plugins.md` says the same in brief.
- `2026-11-03-plugin-vrm.md` points here instead of "Nothing ever cleans
  it", and its "Not done" entry is gone.

## Tests

`texture_cache::tests` and two in `materials::tests`:

- names the plugin writes, and ones it does not;
- age, budget (least recently used first), in-use files kept past the
  budget, a time ahead of the clock, stale and fresh temporary files;
- other files, a directory and a link under a cache name, and the lock
  file stay;
- a held shared lock skips the sweep, and an open waits for a sweep;
- 4 opening threads and 2 sweeping threads, each with its own lock file
  handle (which `flock` keeps apart as it does processes), over images that
  are past the age and a budget of 0: every path an open answers holds its
  bytes after the open, over 20 rounds;
- reopening a model marks its images used, and an open removes another
  model's stale image while keeping its own and a file of the user's.

## Checks

```sh
cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --locked
cargo clippy --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --all-targets --locked -- -D warnings
(cd plugins && cargo fmt --check)
```

## Not done

- The limits are constants, not settings.
- The race test uses threads in one process, not several processes; the
  locks are per open file in both cases.
- On a file system without file locks the sweep never runs, and the cache
  grows as before.
