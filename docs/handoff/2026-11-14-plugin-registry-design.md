# Plugin registry (registry.celesta.studio): design

Status: design, agreed with the maintainer on 2026-11-14. Nothing here is
built yet. Items marked **(proposal)** are the designer's calls, not the
maintainer's, and may be corrected.

## Intent

Celesta gets an official place to publish plugins: `registry.celesta.studio`.
The first release carries only first-party plugins; the end state lets
anyone with a GitHub account publish, with little gatekeeping. Users find,
install, and update plugins from both the command line and the desktop app,
and never place plugin folders by hand. Plugins are installed into one
per-user location, and each project pins the versions it uses, so a project
renders the same on another machine or a year later, as long as it is the
same Celesta minor version (see Project files).

Today plugins arrive only as folders the user copies into the user plugin
folder (or `--plugin-dir`, `CELESTA_PLUGIN_PATH`), the optional
`ffmpeg-x264` is a ZIP on a GitHub release, `vrm` has no distribution at
all, and `@celesta.studio/voicevox` and friends ship inside the CLI runtime
and update only with the app.

npm and crates.io are the reference: versions are immutable, yanking works
as in crates.io, and publishing is self-service.

## Goals

- One registry for every kind of Celesta extension: media plugins
  (`celesta-codec-*`, such as `ffmpeg-x264`), content plugins with a native
  program (such as `vrm`), and component-only plugins (a `module` only, such
  as the migrated `voicevox`). A future kind (a new manifest `role`) needs
  no change to the registry or the client's install path.
- Projects declare their plugins and lock exact versions; opening a project
  on another machine fetches what is missing.
- Four states: Published, Deprecated, Yanked, Blocked. Blocked stops a
  plugin from running even when it is already installed.
- Search, browse, install, and update inside the desktop app.
- Self-service publishing for third parties, authenticated with GitHub, in a
  later phase, with formats ready for it from day one.

## Non-goals

- Human review before a version becomes visible. Publishing is automatic;
  Blocked is the safety net.
- Plugin-to-plugin dependencies in the first release. A plugin depends only
  on Celesta itself (its manifest's `celesta` range). The index format leaves
  room for dependencies later.
- Publishing from anything but GitHub Actions in public repositories (build
  provenance requires it; see Security).
- Running plugins in a sandbox. Plugins stay arm's-length processes as today.
- Paid plugins, licensing keys, download statistics beyond simple counts.

## Anti-goals

These would make the work a failure even if it technically works:

- A project that rendered yesterday stops rendering because a registry
  version was yanked or deprecated. (Blocked is the one deliberate
  exception.)
- Celesta reports to the registry which plugins a user has installed or
  uses. Status checks download whole public files; nothing about the user's
  set is sent.
- A plugin installed from the registry runs without the user ever having
  decided to run it: either they trusted the project that locks it, or they
  installed it themselves.
- Celesta's core starts naming first-party plugins again (see
  `2026-11-13-core-without-first-party-plugin-names.md`). The registry client
  treats `vrm` and `natsuneko.toon-shader` the same way.
- Blocking is slow or can be bypassed by staying offline.

## Constraints from today's code

- A plugin is a directory with `celesta-plugin.json` (`id`, `version`,
  `protocol`, `celesta` range, optional `roles`, `executable` per
  `<os>-<arch>` or OS, `module`, `types`, `components`, `media`); see
  `2026-11-01-plugin-system.md` and `2026-11-05-codec-plugin.md`. Every kind
  in Goals is already this one format: the kind is derived from `roles`,
  whether `executable` names a program, and whether `module` is set.
- `id` is lowercase ASCII letters, digits, `-`, `_`, `.`, starting with a
  letter or digit (`is_valid_id`). It is one import specifier segment
  (`@celesta-plugin/<id>`) and one path component (`.celesta/plugins/<id>/`).
- Search order today (`celesta_plugin_host::search_paths`): `--plugin-dir`,
  `CELESTA_PLUGIN_PATH`, project folders (`plugins.paths`), the user plugin
  folder, added folders, the bundled folder. The first plugin of an id wins,
  so only one version of an id can run.
- Plugins from the user folder, added folders, and projects need approval:
  `settings.json` allows an id in a directory with a fingerprint (SHA-256
  over the manifest and every file it names; `fingerprint`). Plugins turned
  off (`disabledPlugins`) give way to the next of their id. Command-line
  tools follow the same approvals (`2026-11-08-plugin-command-line-approval.md`).
- Projects must be trusted before their code runs (`2026-11-02-plugin-trust.md`).
- Media ranking: origins rank (decoders: user, bundled, project, built-in;
  defaults: built-in, bundled, user, a project's never), then
  `media.priority` within an origin.
- Unix folders are permission-checked (`unsafe_reason`); a store the
  registry client writes must pass it.
- The website is a Cloudflare Worker (`packages/website/wrangler.jsonc`).
- Tests: Vitest in TypeScript for JS/TS, `cargo test` for Rust.
- Every user-facing change updates `packages/website/content/{en,ja}/` and
  `skills/celesta/`; examples a feature supersedes are updated in the same
  pull request; rendering or UI changes attach before/after screenshots.

## Approaches

Three ways to build the registry were considered:

- **A. A Git repository as the index** (Homebrew taps, the old
  crates.io-index). Publishing and state changes are pull requests; CI
  uploads archives to R2 and emits static index files. Cheapest and fully
  audited by Git history, but publishing for third parties would be a pull
  request queue, so an API is needed eventually anyway.
- **B. An API service from the start** (crates.io, npm): Workers + D1 + R2,
  OIDC publishing, search and admin APIs, a web UI. Built once for the end
  state, but in full it delays the first official plugin by the time
  accounts, scanning, and UI take.
- **C. Fix the client-facing format first, start with A's backend, switch
  to B's later.** The switch itself would cost a migration script, rewriting
  the official publish workflow, and the discarded CI (an estimated 2-3
  days of human work).

Also dropped: publishing plugins as npm packages (per-platform
`optionalDependencies`, as esbuild does) or as bare GitHub releases. Neither
lets Celesta force Blocked, own the `<publisher>.<name>` namespace, or treat
all plugin kinds as one format.

**Chosen (maintainer, 2026-11-14): B, built in phases, with the
client-facing files fixed first.** A minimal B costs about the same as C's
first phase and avoids C's migration. The order:

1. **Format and minimal server.** Specify the client-facing files (below),
   build the Worker with only what first-party publishing needs (OIDC from
   `mika-f/Celesta`, admin state changes), and publish the first-party
   plugins.
2. **In parallel, once the format is fixed:**
   - the client (lockfile, store, resolver, CLI, desktop app) and the
     migration of four packages to plugins;
   - full B (GitHub sign-in, publishers, Trusted Publishing for any
     repository, provenance checks, malware scanning, search API, web UI,
     advisories).

   The two live in different repositories (see Repositories) and meet
   only at the format, which this repository defines.

## Plugins and names

- **A registry package is one plugin directory**, unchanged in shape. Its
  kind (media, content with a program, component only, or a future role) is
  derived from the manifest; the registry stores the manifest's `roles`,
  whether it has a program, and whether it has a module, for display and
  filtering only.
- **Names.** First-party plugins keep short ids without a dot (`vrm`,
  `ffmpeg`, `voicevox`); only the Celesta team can register a dotless name.
  Third-party ids are `<publisher>.<name>`, which the current id rules
  already allow, so the manifest format does not change. Examples:
  `@celesta-plugin/natsuneko.toon-shader`.
  - `<publisher>` is a GitHub user or organization login, lowercased, bound
    to the account's numeric GitHub id, so a later GitHub rename does not
    change it and a new owner of the old login does not inherit it
    **(proposal)**.
  - `<name>` uses the id characters except `.`.
  - Publisher names starting with `celesta` are reserved for the Celesta
    team, whoever owns the GitHub login **(proposal)**.
  - The registry refuses a version whose manifest `id` is not the registered
    name **(proposal)**.
- **Names never change.** A plugin that wants a new name is Deprecated with
  a `replacement` pointing at the new id; the app offers to switch
  **(proposal)**.

## States

States are per version, except Deprecated, which may also apply to a whole
plugin.

| State | Who sets it | New resolution | Locked or installed copies | Download |
| --- | --- | --- | --- | --- |
| Published | publishing | chosen | run | yes |
| Deprecated | publisher, admin | still chosen, with a warning | run, with a warning | yes |
| Yanked | publisher, admin | skipped | run | yes (as crates.io) |
| Yanked, withdrawn | admin | skipped | run | no: archive removed |
| Blocked | admin only | skipped | **do not run** | no |

- **Deprecated** carries a message and an optional `replacement` id. It is
  reversible.
- **Yanked** is reversible ("unyank"), like crates.io. A legal or license
  takedown is a yank with the archive withdrawn **(proposal)**: installed
  copies keep running, a fresh install reports "no longer distributed". No
  fifth state.
- **Blocked** is for malware and vulnerabilities only. It is published as an
  **advisory**: plugin id, an affected version range (`<0.2.3`), a
  description, an optional "fixed in" version, and the directory
  fingerprints of the affected builds. It stops the plugin without asking,
  shows the user what was stopped and why, and offers the fixed version when
  there is one. There is no "run anyway"; a false positive is undone by the
  registry withdrawing the advisory **(proposal)**.
- Before Published, a version is **Scanning** (full B only): invisible in the
  index until the automated checks pass. This is not a user-facing state.

## Client-facing files (the contract)

Everything the app and CLI read is static, served from R2 through the
Worker at `https://registry.celesta.studio`, CDN-cacheable, and signed.
This contract is fixed in phase 1 and must not change when the server
grows.

| Path | Contents | Caching |
| --- | --- | --- |
| `/config.json` | format version, API base URL, the current signing keys (each signed by the root key) | short |
| `/index/<id>.json` | one plugin: publisher, kind, description, homepage, source repository, deprecation, and every version: `version`, `celesta` range, `protocol`, `published`, `yanked`, `withdrawn`, `deprecated`, source commit and workflow, and per target `{ url, size, sha256, fingerprint }` | short, revalidated |
| `/search.json` | every plugin's summary (id, publisher, kind, description, latest version, deprecation), for search and browsing in phase 1 and offline | short |
| `/advisories.json` | every Blocked advisory, with `generated` timestamp | short |
| `/archives/<id>/<version>/<target>.zip` | the plugin directory | immutable |

- **Targets** are the manifest's `<os>-<arch>` keys (`macos-aarch64`,
  `windows-x86_64`, `linux-x86_64`, ...); a plugin without a program has the
  one target `any`. A target archive contains the plugin directory at its
  root with that target's executable and no other target's, so `executable`
  still resolves through the existing host rules. Other files are allowed:
  native libraries (a media plugin's `Frameworks/` dylibs or bundled DLLs),
  licenses, notices. Archives keep file modes, as
  `scripts/package-media-plugins.py`'s ZIPs already do.
- **`fingerprint`** is a **tree hash** over every file in the extracted
  directory (relative path, mode, length, contents), not today's approval
  fingerprint, which covers only the manifest and the files it names and so
  leaves bundled native libraries unchecked **(designer's call)**. The
  registry computes it after extracting each archive. It lets the client
  recognize a registry build anywhere: in the store, in a hand-placed
  folder, or bundled in the app. Today's approval fingerprint for
  hand-placed plugins is unchanged, so no saved allowance is asked about
  again; the client computes the tree hash of a hand-placed folder only to
  match advisories.
- **Signing**: each JSON file has a detached Ed25519 signature
  (`<file>.sig`) by an online key; online keys are listed in `/config.json`
  and signed by an offline root key whose public half is compiled into
  Celesta. Clients refuse a file whose signature fails and refuse an
  `advisories.json` older than the one they have (rollback/freeze
  protection). The exact envelope is left to the builder; it must allow key
  rotation without an app update.
- **Schema**: the files are specified once as JSON Schema with shared test
  fixtures, kept in this public repository. The Rust client here and the
  service's TypeScript implementation are both tested against them.
- **Reserved**: a version entry may later gain `dependencies`; clients
  ignore unknown fields.

## Repositories: what is public

- **`mika-f/Celesta` (public)** owns everything a client, a plugin author,
  or a reader of the protocol needs:
  - the client-facing format: its JSON Schema and shared test fixtures, as
    the single source of truth (in this repository; the exact folder is the
    builder's call);
  - the Rust client (store, lock, resolver, advisories, signature checks)
    and the `celesta` command, including `celesta plugin publish`;
  - the publish API's contract (below) and a reusable GitHub Actions
    workflow for publishing, so third parties publish with public tools;
  - the first-party release workflow, which publishes through that API
    with OIDC and holds no registry secret;
  - the user guide (en/ja) and the skill.
- **The registry service's own repository (private, not named here)** owns
  the service: the
  Worker and its API, D1 and R2, the website at `registry.celesta.studio`,
  GitHub sign-in, scanning, the admin path for yank, withdraw, and
  advisories, the online signing keys and their rotation, and its own
  design notes. It takes the schema and fixtures from this repository
  (public, so its CI can fetch a pinned commit) and tests its TypeScript
  implementation against them.
- Nothing from the private repository (its code, internal endpoints, admin
  procedures, secrets, or operational details) is copied into this one, its
  handoff notes, pull requests, or issues. This note describes the service
  only as far as clients and publishers see it.
- The compiled-in root public key is public by nature; the root private key
  is held offline by the maintainer and lives in neither repository.

## Publishing (public contract)

- `POST /api/v1/publish` accepts one version: the manifest and its target
  archives. It is authenticated by a GitHub Actions OIDC token. In phase 1
  only `mika-f/Celesta` may publish; in phase 2, the repository and
  workflow a plugin's owners registered (Trusted Publishing). There are no
  long-lived API tokens.
- The registry validates the manifest as the host does (id, contained
  paths, `celesta` range, the id matching the registered name) and each
  archive's shape (the plugin directory at the root, the target's
  executable present, no other target's executable, no links leaving the
  directory), enforces size limits, computes tree hashes, and regenerates
  the plugin's index, `search.json`, and their signatures. Versions are
  immutable: republishing a version is refused.
- From phase 2 every archive must carry a GitHub artifact attestation
  (Sigstore) from the registered repository and workflow, which in turn
  requires a public source repository. A version stays invisible until the
  automated checks (static checks, then ClamAV and YARA) pass; there is no
  human review.
- Yank, unyank, and deprecate are available to a plugin's owners (phase 2:
  through the registry website, signed in with GitHub). Withdrawal and
  advisories are the Celesta team's alone.
- First-party publishing: a release workflow in this repository builds each
  plugin per target (reusing `scripts/plugins/build.mts` and the codec
  packaging, macOS signing and notarization included), attests each archive
  (`actions/attest-build-provenance`), and publishes. The bundled `ffmpeg`,
  `avfoundation`, and `mediafoundation` are published too, so projects can
  pin them; they stay bundled in the app so a fresh install exports offline.
- **Bundled copies are the published archives, byte for byte**
  **(designer's call)**. A release publishes the plugins first; the app
  packaging then extracts the published archives into the bundled folder
  instead of building and signing those plugins itself. On macOS the
  archives' Mach-O files are already signed with the Developer ID and
  notarized (as the optional codec ZIPs are today), so the app's signing
  must leave them as they are (no `--force` over nested plugin code). This
  way a bundled copy's tree hash equals the registry's: advisories reach it,
  and a lock on `ffmpeg 0.9.0` is satisfied by the bundled copy offline.
  This changes the order of the release pipelines (`package-build.yml`,
  `package-macos.py`, `package-windows.ps1`).

## The service, as clients see it

- Phase 1 (minimal): first-party publishing only, the client-facing files,
  and the Celesta team's yank, deprecate, withdraw, and advisory changes.
- Phase 2 (full): GitHub sign-in, publishers as GitHub users and
  organizations, owners, Trusted Publishing for any repository, provenance
  checks, scanning, a search API (with `search.json` as the offline
  fallback), the website (search, plugin pages, the publisher dashboard),
  reports, and rate limits.
- How the service implements this is designed in the registry repository.

## Client

### Project files

- `celesta.config.json` gains `plugins.dependencies`, a map of id to a
  version requirement with Cargo's semantics (`"0.2"` means `^0.2`)
  **(proposal for the field name and shape)**:

  ```json
  {
    "plugins": {
      "dependencies": {
        "vrm": "0.2",
        "voicevox": "0.9",
        "ffmpeg-x264": "=0.9.0"
      }
    }
  }
  ```

- Content and component plugins a composition imports belong here. Media
  plugins appear here only when the project chooses to pin one (`add
  --pin`); otherwise they are global tools (below).
- `celesta.lock` beside it records, for each dependency, the exact version,
  the registry, and every target's `sha256` and `fingerprint`, so one
  lockfile works on every OS **(proposal for the name)**. It is written by
  the tools, committed by the user, and never edited by hand.
- **The lock records the Celesta version it was resolved with**
  (maintainer, 2026-11-14). The reproducibility promise is: the same lock
  renders the same within the same Celesta minor version. While Celesta is
  0.x, first-party plugins keep following Celesta's minor version
  (`scripts/plugins/build.mts` gives them `>=0.X.0, <0.(X+1).0`), so a
  Celesta minor upgrade leaves every locked first-party plugin outside its
  range. Then:
  - the app says the project was locked with Celesta 0.X, lists the
    plugins that cannot load, and offers one action to re-resolve them
    (`update` restricted to those ids), which rewrites the lock with the
    running Celesta's version;
  - `celesta plugin install --locked` and `celesta-exporter` fail, naming
    the locked Celesta version, the running one, and `celesta plugin
    update`;
  - plugins whose range still includes the running Celesta stay as locked.

  A lock from a newer Celesta than the running one is read when its format
  is understood and fails otherwise, naming the version.
- An entry without `celesta.config.json` has no dependencies: it uses the
  newest version in the store of each id it imports, skipping Blocked ones
  **(proposal)**.

### Store and discovery

- Registry plugins are extracted into a store beside the user plugin
  folder, one directory per version: `<user data>/Celesta/registry/<id>/<version>/`
  on each OS (the parent of today's `user_plugin_dir`) **(proposal)**. It is
  created 0700 on Unix so `unsafe_reason` accepts it. Downloads go to a
  temporary directory, are checked against the lock's or index's `sha256`,
  extracted with the host's containment rules (no `..`, absolute paths,
  links leaving the folder), fingerprinted, and renamed into place.
- Discovery gains a source for the store. For an id the project locks, the
  locked version from the store ranks before every other source, including
  the bundled folder (so a pinned `ffmpeg 0.9.0` replaces a bundled
  0.9.2; versions from another Celesta minor cannot load at all, see the
  lock above). For media plugins, a pin only chooses which version of that id
  runs; which encoder or muxer is the default still follows the existing
  origin and `media.priority` order, with the pinned one taking its id's
  place. A pinned plugin with no build for the running OS is skipped with a
  warning and the global version is used; an export preset that names its
  component then fails.
- **Global plugins** (codecs installed without a project) are store
  versions listed in `settings.json` (`registryPlugins`: id and version)
  and rank where the user plugin folder does today, before bundled ones
  **(proposal)**.
- **Approval.** A store plugin runs without the approval prompt when its
  directory's fingerprint equals the one recorded at install, and either the
  user installed it globally or a trusted project locks it with that
  fingerprint. Opening an untrusted project asks for trust first, then
  offers to download. Hand-placed plugins keep today's approval flow.
- **Advisories.** The client fetches `/advisories.json` at app start, every
  24 hours while running, and once per CLI invocation (a 3-second timeout),
  verifies it, and caches it. Offline, the cached file still applies. A
  plugin matching an advisory, by id and version for store plugins and by
  fingerprint for any folder (hand-placed copies and the bundled ones
  included), does not run anywhere: the editor, exporter, MCP server, and
  React runtime treat it as turned off. The app shows a notification naming
  the plugin and the advisory, and offers the fixed version; a bundled one
  says to update Celesta.
- **Deprecated and Yanked** show in Problems and on the CLI's stderr; a
  replacement is offered when given.

### Resolution

- `add` and `update` resolve each requirement to the newest version that
  is Published or Deprecated (never Yanked or Blocked), whose `celesta`
  range includes the running Celesta, and that has a build for every
  **project target** (or `any`) **(designer's call)**. Project targets are
  `plugins.targets` in `celesta.config.json` when given, else every target
  Celesta's own releases ship for (today `macos-aarch64`, `windows-x86_64`,
  `linux-x86_64`). If no version covers them all, the newest version with a
  build for the running target is chosen, with a warning naming the targets
  it lacks. The lock records the targets it was resolved for. Pinned media
  plugins are exempt (an OS-specific codec such as `avfoundation` falls back
  to the global version elsewhere, as above).
- A store plugin is found by its tree hash: a bundled copy whose tree hash
  equals the locked one satisfies the lock without a download.
- If a locked version is missing from the store, it is downloaded as
  locked, even when yanked; a withdrawn or blocked one fails with the
  reason.
- Indexes are cached; a project whose lock the store already satisfies
  opens with no network access.

### Command line

Celesta has no general `celesta` command today (only `celesta-exporter`,
`celesta-editor`, `celesta-mcp`). The plugin commands go into a new
`celesta` binary, room for later subcommands, shipped beside the others
(name confirmed by the maintainer, 2026-11-14; the subcommands are a
proposal):

| Command | Does |
| --- | --- |
| `celesta plugin search <text>` | search the registry |
| `celesta plugin info <id>` | versions, states, targets, source |
| `celesta plugin add <id>[@<req>] [--pin]` | add to the project, resolve, lock, install; `--pin` for a media plugin |
| `celesta plugin remove <id>` | remove from the project and the lock |
| `celesta plugin install [--locked] [--offline]` | install what the lock names; `--locked` fails rather than change the lock |
| `celesta plugin install --global <id>[@<req>]` | install a global plugin (codecs) |
| `celesta plugin update [<id>]` | re-resolve within the requirements and rewrite the lock |
| `celesta plugin list` | the project's and global plugins with their states |
| `celesta plugin audit` | check the lock and global plugins against advisories, deprecations, and yanks |
| `celesta plugin gc` | remove store versions nothing uses |

- `celesta-exporter` and the MCP server never change the lock or download.
  A project whose lock is not satisfied fails with a hint to run `celesta
  plugin install` **(proposal)**.

### Desktop app

- The Plugins tab (right dock) lists **This project** (locked plugins, with
  available updates and states), **Global** (codecs, bundled ones marked),
  and **Installed by hand** (today's list and approval), plus **Find
  plugins…**.
- **Find plugins…** opens a plugin page in the Assets area, as the export
  page does: search, the plugin's details (kind, targets, versions, states,
  source, advisories), and **Add to this project** (or **Install** for a
  codec, **Pin to this project** as an option).
- Updating a plugin rewrites the lock and reloads the project, so the
  preview shows the new version at once.
- Opening a project whose lock is not satisfied asks once, listing what it
  will download (ids, versions, sizes), after the trust question.
- Notifications for Blocked plugins; Problems entries for Deprecated and
  Yanked ones.
- The visual design follows the GPUI Kit Design Guides and is settled with
  mockups when this part is built.

## Migrating four packages to plugins

- `@celesta.studio/aivis-speech`, `@celesta.studio/code`,
  `@celesta.studio/lottie`, and `@celesta.studio/voicevox` become
  component-only first-party plugins `aivis-speech`, `code`, `lottie`, and
  `voicevox`, imported as `@celesta-plugin/<id>`.
- Their modules are bundled into one ESM file each, with only `react` and
  Celesta's own packages (`@celesta.studio/react`, `@celesta.studio/shapes`,
  `@celesta.studio/character`, ...) left external, so the CLI resolves those
  to the runtime's instances. `code`'s `@twinkleplop/*` dependencies are
  inside its bundle. All four use only public `@celesta.studio/*` APIs today.
- The old `@celesta.studio/*` imports keep working for a transition, still
  bundled in the runtime, and warn (Problems, stderr) with the new import
  and the `celesta plugin add` command. The packages are private, so this is
  Celesta's own deprecation, not npm's. The old imports are **not removed**
  (maintainer, 2026-11-14): they stay bundled with their warning. The core
  `@celesta.studio/*` packages (`react`, `math`, `shapes`, `character`, and
  the rest) are not affected and stay bundled without a warning.
- Every example that imports them (today `delta`, `font-stack`,
  `shorts-explainer`, `shorts-tips`, `spectra`, `versus`, `zunda`, and
  `plugins/beep-voice`), the user guide in both languages (including the
  `04-(packages)` pages), and the skill move to the new imports and to
  `plugins.dependencies` in the same pull request as the migration.
- `vrm` calls `registerHostElement` from `@celesta.studio/react/internal`.
  `2026-11-01-plugin-system.md` asks for a public plugin-layer API before
  any plugin is published; that API comes before `vrm` is published.

## Security summary

- Integrity: archives checked by `sha256` from a signed index or the lock;
  the directory fingerprint checked at load.
- Authenticity: Ed25519 signatures with a compiled-in root key and rotating
  online keys.
- Provenance: GitHub artifact attestations required for every archive in
  phase 2 (and produced for first-party archives from phase 1).
- Malware: static checks, then ClamAV and YARA (phase 2); Blocked
  advisories, enforced by fingerprint, even offline.
- Privacy: whole-file downloads only; no per-user requests.
- Extraction: the host's existing containment rules; Unix permission checks
  on the store.

## Decided by the maintainer

- Projects pin versions; a global store keeps every version.
- Codecs are global tools by default; a project may pin one, and a pin
  replaces that id's version everywhere in that project.
- Blocked is for malware and vulnerabilities; license takedowns use Yanked.
- Ids are `<publisher>.<name>` for third parties.
- `aivis-speech`, `code`, `lottie`, and `voicevox` move to the registry; the
  old packages are deprecated.
- The app searches, browses, installs, and updates.
- No human review; cheap automated malware checks.
- GitHub is the only sign-in.
- The lock records the Celesta version; identical output is promised
  within one Celesta minor version, and an upgrade offers to re-resolve.
  First-party plugins keep following Celesta's minor version during 0.x.
- Approach B in phases, format first, then client and full server in
  parallel.

## Settled after review

- The four deprecated imports are never removed; core packages stay.
- The new command is `celesta`.
- The server and website live in a separate, private repository;
  `mika-f/Celesta` is public. The format, the client,
  the publish contract, and the publishing tools are public; the service's
  internals stay private.

## Left to the builder

- The signature envelope and key-rotation details, within the constraints
  above.
- The folder in this repository that holds the schema and fixtures.
- Cache lifetimes and retry behavior.
- Everything inside the service, designed in the registry repository.
- How the store records install metadata (fingerprint at install, source).

## Checks this work needs

- The format: the Rust implementation validated against the schema's
  fixtures (`cargo test`); the service runs the same fixtures in its own
  repository.
- Publishing: `celesta plugin publish` and the reusable workflow build
  archives the publish contract accepts (archive shape, one target's
  executable, modes kept), tested locally without the service.
- The client: resolution (yanked skipped but locked ones kept, `celesta`
  range, targets), lock round trips, store extraction safety (zip-slip,
  links, modes), signature and rollback refusal, advisories by version and
  by fingerprint (hand-placed and bundled), offline behavior with a cached
  advisory file, approval of locked plugins in trusted and untrusted
  projects, pinned media plugins replacing bundled ones without changing
  defaults.
- The app: screenshots of the Plugins tab and the plugin page before and
  after; the missing-plugins question; the Blocked notification.
- The migration: the four plugins load from the store; the old imports
  still render, with the warning; examples type-check.
