# Plugin registry client core (2026-11-14)

Plan 3 of the plugin registry. The design is
`docs/handoff/2026-11-14-plugin-registry-design.md`; the file formats are in
`docs/handoff/2026-11-14-registry-format.md` and `crates/registry-format/README.md`.
This plan adds the client core: projects declare and lock registry plugins, a
per-user store holds verified copies, a resolver picks versions, signed index and
advisory files are fetched and cached behind a transport trait, and discovery
runs locked and global store plugins and stops plugins under an advisory from
every source. Load-time pieces live in `celesta-plugin-host`, because the editor,
exporter, MCP server, and React bridge already discover plugins through it. The
network-facing pieces live in the new crate `celesta-registry-client`
(`crates/registry-client`). Dependencies run one way: `registry-client` to
`plugin-host` to `registry-format`. Nothing in this plan calls the network from
the editor, exporter, or MCP server.

## Specification

Decisions this plan made (the maintainer confirms them):

1. `celesta-registry-format` is publishable (`publish = false` is removed),
   because the published `celesta-plugin-host` now depends on it. Before the next
   release the maintainer must publish a `0.0.0` placeholder for
   `celesta-registry-format` and run `scripts/crates-trusted-publishing.sh`, since
   the release workflow publishes every crate without `publish = false` (see
   `docs/handoff/2026-11-14-crates-io-trusted-publishing.md`).
   `celesta-registry-client` keeps `publish = false`; it needs the same setup once
   the editor depends on it (plan 5).
2. Rollback stamps are a format change: `config.json` and `index/<id>.json` gain a
   required `generated` timestamp (`search.json` and `advisories.json` already had
   one). `check_fresh` applies the rule. The registry service syncs fixtures from a
   pinned commit, so it must move the pin and emit the field; a note for the
   service's session was left.
3. Search order is: Explicit (`--plugin-dir`, `CELESTA_PLUGIN_PATH`), Locked,
   Project, User, Registry (global), Folder, Bundled. Locked comes after Explicit,
   because an explicit folder is the caller's choice for one run and plugin
   developers test a build that way.
4. A plugin entry without `celesta.config.json` uses global registry plugins only,
   not the newest store version of each id: a store version installed only for
   another project was never chosen to run here.
5. Install metadata lives in what chose the plugin: the lock records each target's
   `treeHash`, and `registryPlugins` in `settings.json` records the global one's.
   The store holds only plugin directories.
6. No root key is compiled in yet. `official_root_key()` returns `None` until the
   maintainer supplies the root public key; every client API takes a `RootKey`, and
   tests use the fixture root.
7. The cache lives at `<user data>/Celesta/registry-cache/`, the store's sibling,
   not in an OS cache folder, because purging a cache would drop the advisory list.

Project declaration. `celesta.config.json` gains `plugins.dependencies` (plugin id
to a version requirement with Cargo semantics, so `"0.2"` is `^0.2` and a
pre-release matches only when the requirement names one) and `plugins.targets`
(`<os>-<arch>` or `any`; default `macos-aarch64`, `windows-x86_64`,
`linux-x86_64`). The manifest paths the tree hash would skip (a last component of
`.DS_Store`, `Thumbs.db`, or `desktop.ini`) are refused.

Lock format. `celesta.lock` sits beside `celesta.config.json`, is committed, and is
written whole and replaced atomically. It carries `format: 1`; a lock with another
`format` fails naming it, and keys this build does not know are kept on rewrite.
It is written with `format` first, then the other keys in alphabetical order,
two-space indentation, and one trailing newline (`the_bytes_are_stable` in
`crates/plugin-host/src/lock.rs` pins the bytes):

```json
{
  "format": 1,
  "celesta": "0.9.0",
  "plugins": {
    "acme.charts": {
      "celesta": ">=0.9.0, <0.10.0",
      "registry": "https://registry.celesta.studio",
      "targets": {
        "any": {
          "sha256": "sha256:<64 hex digits>",
          "treeHash": "sha256:<64 hex digits>"
        }
      },
      "version": "1.2.3"
    }
  },
  "targets": [
    "linux-x86_64",
    "macos-aarch64",
    "windows-x86_64"
  ]
}
```

`sha256` is the archive as downloaded; `treeHash` is the installed folder.

Approval rule. A store folder runs without the approval prompt, and only while its
tree hash is the one recorded at install: the lock's entry for this host in a
trusted project, or the one in `registryPlugins` in the user's `settings.json`. A
lock in an untrusted project is not read. A locked plugin needs the lock's `celesta`
range to match the running Celesta (`Lockfile::unloadable` lists those that do not).
A locked store copy that fails its check (tree hash, id and version, or another read
error) still lets a bundled copy with the locked id, version, and tree hash satisfy
the lock; `LockUnsatisfied` then explains why the store copy did not (its files
differ, or it does not load with this Celesta).

Advisories. Discovery reads the cached, verified list without the network; a
plugin it names stops from every source, including explicit paths and bundled
copies. A cached verified list stays in force offline and after its key expires. A
list more than 10 minutes in the future and one older than the cached list are
refused; a warning is due when the newest verified list is older than 7 days
(`advisories_stale`). An unusable cache is handled in two ways. Discovery reports
`DiscoveryError::Advisories` and runs without the list (it fails open, as
specified, so a damaged file does not stop every plugin). The registry-client
project operations instead fail with `ClientError::Cache`, so they never install a
blocked version. An unreadable lock is `DiscoveryError::Lock`.

Store layout and extraction. The store is `user_plugin_dir()`'s parent joined with
`registry`, one folder per version at `<id>/<version>`; the cache is its parent
joined with `registry-cache`. Both are created 0700 on Unix. An install downloads
to a `.tmp-*` folder in the store, reads at most one byte more than the listed
size, checks the size and SHA-256, then extracts. Every entry must be a folder or a
regular file with a plain relative name (no `\`, `:`, NUL, empty, `.`, or `..`
part), named once; symlinks and other types are refused; entries and unpacked bytes
are counted against limits (100,000 entries, 4 GiB). The unpacked tree hash must
equal the listed one and the manifest must carry the requested id and version before
the folder is renamed into place. A version already there with the right tree hash is
kept without a download; a tampered one is replaced. A crash leaves a `.tmp-*`
folder, which discovery ignores.

Resolver rules. For each dependency in id order: a previous lock entry is kept when
nothing asks for it to move, the requirement and its `celesta` range still match,
and it is not blocked or withdrawn (a yanked one is kept with a warning). Otherwise
the newest version is chosen that matches the requirement and the running Celesta
and is neither yanked, withdrawn, nor blocked. A non-media plugin must cover every
project target (a target or `any`); failing that, the newest version covering the
host is chosen with a warning naming the missing targets; a media plugin takes the
newest match, since its builds are per OS. `Update` is `Keep`, `All`, or `Only(ids)`.
Deprecations are warned about.

Freshness. The cache records, per served path, the newest `generated` verified
there, and a served file older than it is refused; the first fetch of a path cannot
be refused. `config.json` and an index are fetched over HTTPS through
`celesta-remote`'s agent (`CLI_ADVISORY_TIMEOUT` is 3 seconds for the CLI's
advisory refresh).

## Implementation

| Path | Responsibility |
| --- | --- |
| `crates/registry-format/src/{files,advisories,tree}.rs`, `schema/v1/{config,index}.schema.json`, `fixtures/` | `generated` on `Config` and `Index`, `check_fresh`, `is_os_junk_name`; `Keyring::from_config` re-derives keys from the raw strings. |
| `crates/plugin-host/src/project.rs` | `plugins.dependencies` and `plugins.targets` (`ProjectDependencies`, `project_dependencies`). |
| `crates/plugin-host/src/lock.rs` | `Lockfile`, `LockedPlugin`, `LockedArtifact`; read, write, and `unloadable`. |
| `crates/plugin-host/src/registry.rs` | `RegistryContext` and `ProjectLock`: the store, cached advisories, and the lock; discovery of locked and global store plugins. |
| `crates/plugin-host/src/advisories.rs` | The cached advisory list, `advisories_stale`, and `blocking_advisory` (applied to every source). |
| `crates/plugin-host/src/{locations,settings,manifest}.rs` | `PluginSource::{Locked, Registry}` and the search order; `registryPlugins` in settings; store and cache paths; the junk-name path refusal. |
| `crates/registry-client/src/transport.rs`, `http.rs` | The `Transport` trait, `DirTransport` (tests), and `HttpTransport` over `celesta-remote`. |
| `crates/registry-client/src/cache.rs`, `registry.rs` | Fetch, verify, and cache `config.json`, indexes, `search.json`, and `advisories.json`; the freshness stamps; `Registry`. |
| `crates/registry-client/src/resolve.rs` | The resolver and its warnings and errors. |
| `crates/registry-client/src/store.rs` | `Store`, `InstallRequest`, and archive installation. |
| `crates/registry-client/src/project.rs` | `lock_project`, `install_locked`, and `install_global`. |
| `crates/registry-client/src/error.rs` | `ClientError`. |
| `HANDOFF.md`, `crates/registry-format/README.md`, the design note | Workspace map rows and the publish line; the design note's status. |

## Checks

Run in this worktree.

| Command | Result |
| --- | --- |
| `cargo test --workspace --locked --no-fail-fast` | Passed: 3108 tests passed, 0 failed, 1 ignored (summed over all test binaries). The timing check `a_process_left_holding_stderr_does_not_delay_the_diagnostic` passed in this run, so no rerun was needed. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings. |
| `cargo fmt --all -- --check` | Passed. |
| `git diff --check` | Passed. |

The build used `PKG_CONFIG_PATH` pointing at Homebrew's `ffmpeg@8`.

Not run: the first-party plugins' separate workspace (`plugins/`), `pnpm` tests, and
the registry service's own tests (a separate repository).

## Later plans

- Plan 4 (the `celesta` command): fetching advisories and passing a project lock to
  discovery, `gc` of the store, the user guide in `packages/website/content/en/` and
  `ja/`, and the Celesta Agent Skill in `skills/celesta/`, which change when the
  command makes this user-facing.
- Plan 5 (the desktop app): the editor depends on `celesta-registry-client`, which
  then needs the crates.io setup of decision 1.
- The maintainer supplies the root public key (decision 6) and does the crates.io
  step of decision 1 before the next release.

## Known gaps

- `locked.registry` is ignored; one registry is assumed.
- The "same build" check exists three times (`plugin-host` `StoreFolder::holds`,
  `registry-client` `Store::contains` and `Bundled::holds`) and should become one
  `plugin-host` helper in plan 4.
- `CLI_ADVISORY_TIMEOUT` applies per phase of each request, so plan 4 must bound the
  whole refresh.
- `tree_hash` reads each file whole, and every discovery re-hashes every store
  plugin; consider streaming or caching.
- `config.json` is re-fetched on each index, search, and advisories call.
- A withdrawn or blocked locked version is re-resolved without a warning, and a
  locked version absent from the index is kept.
- Windows-hazard names (a trailing `.` or space, device names) are refused only
  through the tree hash.
- `.tmp-*` folders are never cleaned (`gc`, plan 4).
- A kept lock entry is not checked against newly added project targets.
- Callers (exporter, MCP, editor, `celesta`) do not yet pass a project lock
  (`find_in` with `RegistryContext::current(Some(root), ...)`) or refresh advisories;
  that is plans 4 and 5.
- The root public key is not compiled in (`official_root_key()` is `None`).
