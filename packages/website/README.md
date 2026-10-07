# Celesta website

An English and Japanese product website built with Vite, React, TypeScript, and Tailwind CSS 4.
It is an independent pnpm package, like `packages/logos` and `packages/react`.

## Develop

Use Node.js 22.12+ (Node.js 24 LTS recommended) and the pnpm version declared in the root `package.json`.

```sh
cd packages/website
pnpm install --frozen-lockfile
pnpm dev
```

Vite prints the local URL. `pnpm build` type-checks the site and writes static
files to `dist/`. `pnpm preview` serves that production build.

## Deploy to Cloudflare Workers

The site uses [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/).
A small `worker.ts` redirects `/` using the browser’s `Accept-Language` header.
Only `/` and `/index.html` run the Worker first; explicit language URLs and
other files are served as static assets through the `ASSETS` binding. No database
or runtime secrets are required. `wrangler.jsonc` names the Worker
`celesta-website` and serves `dist/`.
Change the name there if your account already uses it for another project.

From `packages/website`:

```sh
pnpm exec wrangler login
pnpm deploy:check
pnpm deploy
```

`deploy:check` builds the website and validates deployment with
`wrangler deploy --dry-run`; it does not publish. `deploy` rebuilds and publishes
to your Cloudflare account. Wrangler prints the deployed URL. Add a custom domain
in that Worker's Settings → Domains & Routes after deployment.

For Git integration in **Cloudflare Workers Builds**:

- Root directory: `packages/website`
- Build command: `pnpm install --frozen-lockfile && pnpm build`
- Deploy command: `pnpm exec wrangler deploy`
- Node.js version: `24`

For a CI runner outside Cloudflare, authenticate Wrangler with the
`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets. Keep tokens out of Git.

Run `pnpm preview:cloudflare` to build and serve the site through local Wrangler.
Vite builds shared website and documentation entries, then `vite.config.ts`
generates a real HTML file for every supported language and chapter. Direct
visits to `/en/`, `/ja/`, `/en/docs/export/`, and `/ja/docs/export/` work without
an SPA fallback. Each page has localized `lang`, title, description, canonical,
and `hreflang` metadata. The Vite dev server and preview also support language
routing. Existing `/docs/` and `/docs/<slug>/` URLs keep the English guide;
legacy `/docs/#<chapter>` links still open the matching chapter.
The included `404.html` handles unknown routes; `public/_headers` defines response
headers and immutable caching for Vite's hashed assets.

## Languages

- `/` redirects with HTTP 302 based on `Accept-Language` preferences and `q`
  weights. Regional tags such as `ja-JP` select Japanese. Missing or unsupported
  languages fall back to English. `User-Agent` identifies the browser and OS,
  so it is not used as a language signal. The response uses
  `Vary: Accept-Language` and `Cache-Control: no-store`.
- `/en/` and `/ja/` always display the requested language, including playground,
  downloads, documentation navigation, search, and every chapter’s prose/API
  tables. Code examples and API identifiers remain unchanged.
- Header language links preserve the current chapter, query string, and anchor.
- `src/locales.ts` is the language registry. `src/locales/en.json` is the source
  catalog; `src/locales/ja.json` contains Japanese translations. `catalog.ts`
  initializes i18next with these resources and checks their nested key structure
  in TypeScript. React copy uses react-i18next’s `Trans`.

To add a language:

1. Copy `src/locales/en.json` to `<language>.json` and translate the values.
   Preserve `{{name}}` interpolation variables and named React slots.
   `<slot0/>` inserts a code example or dynamic value; `<slot0>…</slot0>`
   preserves an element such as a link or emphasis while allowing its text
   and position to change. `text(key, [component0, …])` supplies these slots
   to `Trans`; missing wrappers preserve their translated children. These
   slots are React elements, not arbitrary HTML.
2. Register a lowercase URL code (such as `ja` or `pt-br`) and native name in `src/locales.ts`, import its catalog in
   `src/catalog.ts`, and add it to `catalogs`.
3. Run `pnpm test` and `pnpm build`. Routing, language links, HTML generation,
   and alternate-language metadata all use the registry automatically.

Edit prose in the catalogs; layout and API examples stay in the TSX files.
Use nested objects for pages, features, and documentation chapters, for example:

```json
{
  "docs": {
    "chapters": {
      "voicevox": {
        "metadata": { "title": "@celesta/voicevox" },
        "intro": "<slot0/> converts a VOICEVOX AudioQuery to a lip-sync track."
      }
    },
    "search": {
      "matches_one": "{{count}} matching topic",
      "matches_other": "{{count}} matching topics"
    }
  }
}
```

Read a leaf with `t('docs.chapters.voicevox.metadata.title')`. For counts, call
`t('docs.search.matches', { count })`; i18next selects the plural form. Keep
source keys in every catalog and add the plural suffixes required by a new
language (`_one`, `_few`, `_many`, `_other`, and so on). Keys are checked against
the English resource type. Tests recursively check completeness, interpolation
variables, and Trans slot nesting. There is no flattened lookup catalog or
custom interpolation/markup parser.

## Download links

The header, hero, start section, and installation guide link to the Celesta
download. By default every button opens the latest GitHub release. Set these
variables at build time to point elsewhere (see `.env.example`):

| Variable | Used for | Default |
| --- | --- | --- |
| `VITE_DOWNLOAD_URL` | "All releases" links and visitors on other platforms | `…/releases/latest` |
| `VITE_DOWNLOAD_URL_MACOS` | "Download for macOS" | `VITE_DOWNLOAD_URL` |
| `VITE_DOWNLOAD_URL_WINDOWS` | "Download for Windows" | `VITE_DOWNLOAD_URL` |

Each value may be a release page or a direct file URL. Locally, put them in
`.env.local`. In Cloudflare Workers Builds, add them as build variables. The
hero picks macOS or Windows from the visitor's user agent and shows a generic
button on phones, tablets, and other systems. Values are baked into the build,
so rebuild after changing them.

The release workflow (`.github/workflows/package.yml`) does this automatically:
once the GitHub release is created, its `deploy-website` job sets the three
variables to the release's macOS (arm64) `.dmg` and Windows (x64) `-setup.exe`
assets and runs `pnpm run deploy`. It needs the `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` repository secrets.

## Content and behavior

- `src/App.tsx`: product copy, navigation, documentation and repository links.
- `src/links.ts` and `src/Download.tsx`: repository and download links, and the
  platform-aware download buttons.
- `/docs/`: the on-site bilingual user guide, one page per chapter at
  `/<language>/docs/<slug>/`, all built from `docs/index.html`. `src/docs-nav.ts` is the
  single list of groups and pages (slug, title, description, search keywords);
  add a page there, then add its content under the same slug in
  `src/docs-content.tsx` (app, React, and guides) or `src/docs-packages.tsx`
  (`@celesta/math`, `@celesta/code`, and `@celesta/voicevox`). `src/Docs.tsx` renders the grouped
  sidebar, topic search, previous/next links, and the overview. Old
  `/docs/#<chapter>` links redirect to the matching page. Link between pages
  with `docPath(slug)`; `src/docs-shared.tsx` has the `Note` and `Api` helpers.
  `src/DocCode.tsx` supplies accessible copy controls for code examples.
  `src/syntax.ts` highlights code with [twinkleplop](https://twinkleplop.pngwn.at)
  (TSX, JSON, and shell) in documentation.
- `src/examples/`: complete documentation examples (including title, media, dialogue, dialogue sequencing,
  fitted text, reusable data, and VOICEVOX entries). The
  docs import them as raw text. They are excluded from the site's own
  type-check and checked against the real `@celesta/react` and the separately staged `@celesta/voicevox` instead.
  The JSON chapter imports `../../examples/editor-demo.celesta.json` from the
  repository so its example stays in sync. Build with the repository present.
- `src/demo/title-scene.tsx`: editable starting composition.
- `src/SourceEditor.tsx`: lazy-loaded Monaco editor via `@monaco-editor/react`.
  Editor and language workers are bundled by Vite and served from the site.
  File switching preserves undo history and scroll; resetting the sample or
  opening a file starts a fresh editor session. Syntax diagnostics are enabled;
  project errors come from the existing compilation worker.
- `src/Playground.tsx`: TSX editor and file/media import. `@celesta/web` provides
  frame preview and MP4 export. esbuild WASM compiles the visitor's code in a
  dedicated Worker, and Celesta's real React reconciler/evaluator produces
  each `Scene`. The browser canvas renderer draws that Scene for both preview
  and export. Mediabunny uses WebCodecs for H.264/AAC and MP4 muxing.
- `src/style.css`: design tokens, responsive layouts, and reduced-motion styles.
- `src/Brand.tsx` and `public/favicon.svg`: the website's crescent mark (a "C"
  with one star). It is separate from the app logos in `packages/logos`.

The home-page editor accepts one self-contained TSX file; showcase entries
provide their full virtual source projects. Runtime imports are limited to
`@celesta/react`, `@celesta/math`, `@celesta/code`, and `react`. Browser-compatible
`prepare()` and font loading work; filesystem preparation, companion JSON
projects, PSD portraits, and other npm imports require the desktop/CLI workflow. Add
local image, video, or audio files with **Add media** and refer to them by file
name in `src`. Remote media needs CORS access. The web preview is silent;
constant-rate/constant-volume audio clips are mixed into the exported MP4.
Animated audio rate/volume reports an error instead of exporting incorrect
sound. Encoding requires the browser's H.264 WebCodecs support, and AAC support
for compositions with audio. Browser canvas text, video seeking, and fonts can
differ from the native renderer, so use the desktop exporter for pixel-exact
native output. Export currently buffers the MP4 in memory.

The website uses packaged macOS and Windows downloads as the primary onboarding
flow. The source-build workflow is a separate developer chapter. Keep
installation copy aligned with the packaging scripts and published release
assets. Showcase compositions may load Google Fonts; the website UI uses
system fonts. There are no analytics requests. Public
links use the repository's configured GitHub origin.

## Verification

```sh
pnpm test
pnpm build
pnpm check:examples
pnpm deploy:check
```

`check:examples` type-checks the playground scene and every file in
`src/examples/` against the declarations staged by `packages/react`'s
`pnpm run build` (`dist/project-types/`). Build that package first.

Check the page at desktop and mobile widths: navigation, TSX changes,
playback/pause/scrubbing, source opening/copying/downloading, media import,
canceling, and MP4 download. After changing the scene, export it and inspect
the MP4 codecs, frame count, and an actual decoded frame. Test
`pnpm preview:cloudflare` for static asset serving and missing-page responses.

For documentation changes, verify topic search (including no results), OS
selection, code copying, mobile contents navigation, and direct visits to
`/docs/export/` and the legacy `/docs/#export` using both Vite and local Wrangler. Keep guide instructions in
sync with the root README and the React API, and run `pnpm check:examples`
after changing any example.

The user guide follows the React workflow in the root README. The JSON timeline
chapter is for compatibility with existing files; new project, preview, and
export instructions use React entries. Keep CLI flags aligned with
`crates/exporter/src/main.rs` and `crates/editor/src/cli.rs`, and package import
examples aligned with each package's public `src/index.ts`. In particular,
VOICEVOX adapters are imported from `@celesta/voicevox`, not `@celesta/react`.

Catalogs currently ship together in the shared JavaScript bundle. This is a
simple tradeoff for two languages; switch to loading a catalog per language if
additional locales make its transfer size significant. Search aliases live in
the catalogs too, so add terms readers use in each language. Locale-specific
404 pages and no-JavaScript fallback links are generated with the other HTML.

## Showcase

`/en/showcase/` and `/ja/showcase/` list the compositions in `examples/`.
Each has its own static HTML address, such as `/ja/showcase/apex/`, with
localized metadata and language links. The header links to the showcase.
Unprefixed `/showcase/` addresses preserve the English entry.

Reel, Apex, Afterimage, Signal, Spectra, and 36 Days run in the browser.
The editor loads their original TSX/TS sources on demand; choose a source
file to edit it, and the full project recompiles. Playback, frame scrubbing,
reset, source downloads, and MP4 export use the same `Playground` as the home
page. Browser editions explicitly omit generated soundtracks. Prism and
Feature Tour need native PSD/voice preparation; Versus needs three generated
benchmark videos. Their detail pages link to the original source and setup
instructions and clearly identify the desktop requirement.

`src/showcase-catalog.ts` defines the works and initial preview frames.
Add descriptions and other UI text to both locale catalogs. The source glob
in `src/showcase-sources.ts` selects the browser-ready example directories.
Vite reads these as editor text, bypassing desktop-only TypeScript configs.
The build copies the shared OpenType fonts, license files, and portrait PNGs
to `dist/showcase-assets/`; development serves the same paths. Keep these
asset paths in sync when adding an example with new media.

Verify all browser-ready entries, a dependent-file edit, reset, playback,
scrubbing, MP4 export, localized direct visits, and the mobile menu. The web
package's `pnpm test` checks virtual module resolution and unavailable imports.
