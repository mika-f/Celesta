# Celesta website

The English and Japanese product website and user guide, built with
[Makit](https://github.com/mika-f/makit) (Markdown documentation and static pages on Next.js)
and a Vite-built browser playground. It is an independent pnpm package, like `packages/logos`
and `packages/react`, deployed to Cloudflare Workers.

## Develop

Use Node.js 22.12+ (Node.js 24 LTS recommended) and the pnpm version declared in the root `package.json`.
`@celesta/web` imports generated types from `@celesta/react`, so run
`pnpm run build:runtime` from the repository root in a fresh checkout and after
the Rust types change.

```sh
cd packages/website
pnpm install --frozen-lockfile
pnpm dev
```

`pnpm dev` builds `@celesta/web` and the playground island, then runs the island's Vite watch
build next to `makit dev`, which prints the local URL. The pages load the island from
`public/_playground/`, so it must be built before `makit dev` starts; if you run `makit dev`
by itself, run `pnpm playground:build` first.

`makit dev` may not notice edits to files included into the documentation (`file=` fences, see
below). Re-save the Markdown file that includes them, or restart `pnpm dev`.

| Command | What it does |
| --- | --- |
| `pnpm dev` | Island watch build + `makit dev` |
| `pnpm build` | Type-check, build the island, `makit build`, then the post-build step; writes `dist/` |
| `pnpm check` | `makit check`: configuration, metadata, and links without building |
| `pnpm test` | Type-check, then run the Vitest unit tests in `tests/` |
| `pnpm check:examples` | Type-check the playground scene and `examples/` against the real packages |
| `pnpm preview` | `makit preview`: serve `dist/` |
| `pnpm preview:cloudflare` | Build, then serve `dist/` through the Worker with local Wrangler |
| `pnpm deploy:check` / `pnpm run deploy` | Build, then validate (`--dry-run`) or publish |

## Structure

```text
packages/website/
├── makit.config.ts        # site, locales, theme, Markdown plugins, llms.txt
├── content/<locale>/docs/ # the user guide: one Markdown file + .meta.ts per chapter
├── pages/                 # Makit static pages: landing (index.tsx), showcase/
│   └── _components/       # shared parts: download button, playground mount, showcase entry
├── theme/                 # Noir, the Makit theme (theme.extends: "./theme")
├── site/                  # shared code: copy/, links, downloads, locales, showcase catalog
├── playground/            # the playground island, built by Vite into public/_playground/
├── examples/              # documentation example sources, included into chapters
├── plugins/               # remark plugins: remark-include, remark-download-links, remark-arrow-icons
├── scripts/postbuild.mjs  # fixes up dist/ after makit build (see below)
├── public/                # favicon, _headers; build output: _playground/, showcase/, showcase-assets/
├── worker.ts, wrangler.jsonc
└── tests/
```

- **`content/`** is Makit's `sourceDir`. Groups are Route Group directories such as
  `02-(guides)/` with a `category.makit.ts` holding the localized group title, so the sidebar
  groups chapters while URLs stay `/<locale>/docs/<slug>/`. Package sub-pages (`math-noise`,
  `math-shaping`, `code-typing`) sit in a nested group after their package page.
- **`pages/`**: `index.tsx` is `/en/` and `/ja/`; `showcase/index.tsx` the gallery; each
  `showcase/<slug>.tsx` renders the shared `ShowcaseEntry` for one work in
  `site/showcase-catalog.ts`. Each page has a `.meta.ts` with its localized title and description.
- **`theme/`**: `index.tsx` exports the slots (root layout, header, footer, docs page, sidebar, page
  header, pager, table of contents, 404, static page); `noir.css` holds the light and dark tokens and maps them onto
  Makit's; `docs.css` styles the documentation.
- **Icons** come from `lucide-react` (`import { ArrowRight } from 'lucide-react'`) in `theme/`,
  `pages/`, and `playground/`; pass `aria-hidden="true"`. Don't put arrow or symbol glyphs
  (`→ ↗ ↓`) in copy or JSX, since some platforms draw them as emoji: render the icon beside the
  string instead. In Markdown, write arrows as usual: `plugins/remark-arrow-icons.ts` turns
  `→ ← ↗ ↓ ↑` outside code into icon spans (`.nr-icon-*`, drawn in `theme/docs.css`).
- **`playground/`**: the editor (`Playground.tsx`, Monaco in `SourceEditor.tsx`), the editable
  demo scene (`demo/title-scene.tsx`), the showcase source loader (`showcase-sources.ts`, which
  reads the repository's `examples/`), and the island's own strings (`copy.ts`).
  `playground/vite.config.ts` writes hashed files to `public/_playground/assets/` and a
  `manifest.json` naming the entry; `pages/_components/PlaygroundMount.tsx` reads the manifest
  and calls `mountPlayground` in the browser. The build also copies showcase posters to
  `public/showcase/` and the showcase's fonts, licenses, and portraits to `public/showcase-assets/`.
- **`examples/`**: complete documentation examples, including the VOICEVOX and AivisSpeech
  entries. They are excluded from the site's type-check and checked by `pnpm check:examples`
  against the real `@celesta/react` and the separately staged `@celesta/voicevox` and
  `@celesta/aivis-speech` instead.

## Adding a chapter

Add the chapter to **both** locales, in the same group directory and position:

1. `content/en/docs/<NN>-(<group>)/<NN>-<slug>.md` and the same path under `content/ja/`.
   The numeric prefix orders the sidebar and is not part of the URL.
2. A `.meta.ts` next to each file with the same `id` (the slug):

   ```ts
   import { definePageMetadata } from '@natsuneko-laboratory/makit/metadata';

   export default definePageMetadata({
     id: 'export',
     title: 'Export a video',
     description: 'Take your composition from the preview to an MP4.',
   });
   ```

   Add `navigation: { title: '…' }` for a shorter sidebar label.
3. Add the slug to the list in `tests/content.test.ts`, which pins every chapter, its order,
   and its group in both locales.

Link between chapters with root-absolute URLs in the same locale, such as
`[Export](/en/docs/export/)` in English and `/ja/docs/export/` in Japanese; `pnpm test` checks
that every `/` link stays in its locale and names an existing chapter. Notes use GitHub alert
syntax (`> [!NOTE]`). Run `pnpm check` and `pnpm test` after editing.

Per-platform steps use Makit's Markdown tabs, starting on macOS; the group label names the tab
list for screen readers. Collapsible content uses toggles, and a set of questions an accordion
(`{multiple}` lets several stay open; `{open}` opens one initially). The outer fence needs more
colons than the inner ones:

````md
::::tabs[Installation platform]{default="macOS"}
:::tab[macOS]

```sh title="macOS dependencies"
brew install ffmpeg@8 pkg-config
```

:::
:::tab[Windows]
…
:::
::::

:::toggle[View the companion React composition]
…
:::
````

Keep the same tabs and toggles in both locales; `pnpm test` compares their counts per chapter.
`pnpm check` warns about an invalid group, a missing label, or a `default` that names no tab.

### Including example files

An empty code fence with `file=` is filled from that file at build time
(`plugins/remark-include.ts`):

````md
```tsx file=examples/media.tsx title="media.tsx"
```
````

Paths resolve against `packages/website`, not the chapter, so `examples/…` is this package's
examples and `../../examples/…` the repository's. Without `title=`, the file name becomes the
title. A missing file, or a path outside the repository, fails the build.

## Copy

Text outside the documentation lives in `site/copy/en.ts` and `site/copy/ja.ts`, shared by
`pages/` and `theme/`. `ja` must have the type of `en`, and `pnpm test` checks that it has every
key. Sentences that contain links or code are functions that receive a wrapper for each slot,
for example `noscript: (s0: Slot) => h(Fragment, null, 'Enable JavaScript, or ', s0('read the docs'), '.')`.
The playground island's strings are in `playground/copy.ts`.

## Languages

- `/` redirects with HTTP 302 based on `Accept-Language` preferences and `q` weights
  (`worker.ts`, `site/locales.ts`). Regional tags such as `ja-JP` select Japanese; missing or
  unsupported languages fall back to English. The response uses `Vary: Accept-Language` and
  `Cache-Control: no-store`. Only `/` and `/index.html` run the Worker; everything else is a
  static asset.
- `/en/…` and `/ja/…` always show the requested language. Unprefixed addresses such as
  `/docs/export/` go through Makit's locale gateway, which picks the browser's language.
- Makit 0.9's root layout writes `<html lang="en">` on every page, so the post-build step
  sets `lang` per locale and rebuilds the Pagefind index, giving each language its own search
  index.

To add a language, add it to `i18n.locales` in `makit.config.ts`, to `Locale` and the catalogs
in `site/copy/`, to `site/locales.ts`, to `playground/copy.ts`, and translate `content/<locale>/`.

## Download links

The header, landing page, and installation guide link to the Celesta download. By default every
link opens the latest GitHub release. Set these variables at build time to point elsewhere:

| Variable | Used for | Default |
| --- | --- | --- |
| `CELESTA_DOWNLOAD_URL` | "All releases" links and visitors on other platforms | `…/releases/latest` |
| `CELESTA_DOWNLOAD_URL_MACOS` | "Download for macOS" | `CELESTA_DOWNLOAD_URL` |
| `CELESTA_DOWNLOAD_URL_WINDOWS` | "Download for Windows" | `CELESTA_DOWNLOAD_URL` |

Each value may be a release page or a direct file URL; an empty value counts as unset. Makit
does not read `.env` files: export the variables in your shell (see `.env.example`) or add them
as build variables in Cloudflare Workers Builds. `site/downloads.ts` reads them while pages
prerender, and in Markdown `celesta:download/macos`, `celesta:download/windows`, and
`celesta:download/all` link URLs are rewritten to them (`plugins/remark-download-links.ts`).
The landing page's button picks macOS or Windows from the visitor's user agent. Values are baked
into the build, so rebuild after changing them.

The release workflow (`.github/workflows/package.yml`) sets them automatically: once the GitHub
release is created, its `deploy-website` job points them at the release's macOS (arm64) `.dmg`
and Windows (x64) `-setup.exe` assets and runs `pnpm run deploy`. It needs the
`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` repository secrets.

## Build

`pnpm build` runs, in order: the `@celesta/web` build, `wrangler types`, `tsc` for the site
(`tsconfig.json`) and for the Worker (`tsconfig.worker.json`, kept apart because workerd's
runtime types redefine DOM globals), the island build, `makit build --clean`, and
`scripts/postbuild.mjs`.

The post-build step works around a Makit 0.9 gap: Makit writes each chapter's Markdown endpoint
(`/en/docs/export.md`) and `llms-full.txt` from the source files without the configured remark
plugins. The step applies `remark-include` and `remark-download-links` to them, editing only the
fences and link URLs those plugins change.

## Makit

`@natsuneko-laboratory/makit`, `@natsuneko-laboratory/makit-runtime`, and `next` are pinned to
exact versions. `lucide-react` is pinned to the version Makit depends on (0.468.0 for Makit 0.9.0):
Makit aliases `lucide-react` for the theme and pages, and the type-check and the Vite island
resolve this package's own copy, so the two must match. Bump them together, then re-check the theme CSS that targets Makit's DOM and
`aria-label`s (`theme/noir.css`, `theme/docs.css`: search dialog, page actions, copy button,
code blocks, tabs and toggles) and the gaps below.

Makit 0.9 gaps this package works around or lives with, besides the post-build fix above:

- **Locale memory** (`localStorage["makit-locale"]`) is only written on a full page load, so the
  `EN / JA` links (`theme/LocaleLinks.tsx`) are plain `<a>` elements that reload the page instead
  of navigating on the client.
- **The 404 page has no locale.** Makit exports one `404.html` and renders its root layout with
  `site.lang`, so `theme/RootLayout.tsx` sets `<html lang>` and `data-nr-locale` from the URL in
  an inline script there, and the page renders every locale's copy for CSS to pick from.
- **Built-in labels stay English** on Japanese pages: the search dialog ("Search documentation…",
  "No results for …") and the page actions ("Edit on GitHub", "Copy as Markdown", …).
  `MakitMessages` has no keys for them.
- **Default parts hard-code rounded corners and a glyph.** The search dialog, page actions,
  copy button, and theme toggle use Tailwind `rounded-*` classes, which `theme/noir.css` and
  `theme/docs.css` override to square corners; each search result ends in a `→` glyph, which
  `theme/docs.css` hides and replaces with a masked lucide arrow.
- **No icon or theme-color option.** `theme/RootLayout.tsx` adds `<link rel="icon">` and
  `<meta name="theme-color">` itself.
- **Code highlighting** accepts only bundled Shiki theme names, and remark/rehype plugins run
  before highlighting, so token colors cannot be adjusted in a plugin. The GitHub high-contrast
  themes reach 4.5:1 on the code panels except one light comment color, overridden in
  `theme/docs.css`.

## Deploy to Cloudflare Workers

The site uses [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/).
`wrangler.jsonc` names the Worker `celesta-website`, serves `dist/`, and uses Makit's themed
`404.html` for unknown addresses (`not_found_handling: "404-page"`). Change the name there if
your account already uses it for another project. `public/_headers` sets security headers and
caches hashed files (`/_next/static/*`, `/_playground/assets/*`) as immutable; the island's
`manifest.json` is revalidated on every request.

From `packages/website`:

```sh
pnpm exec wrangler login
pnpm deploy:check
pnpm run deploy
```

`deploy:check` builds and validates with `wrangler deploy --dry-run`; it does not publish.
`deploy` rebuilds and publishes. (`pnpm deploy` is a built-in pnpm command, so use `run`.) Add a
custom domain in the Worker's Settings → Domains & Routes after deployment.

For Git integration in **Cloudflare Workers Builds**:

- Root directory: `packages/website`
- Build command: `pnpm install --frozen-lockfile && pnpm build`
- Deploy command: `pnpm exec wrangler deploy`
- Node.js version: `24`

For a CI runner outside Cloudflare, authenticate Wrangler with the `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` secrets. Keep tokens out of Git.

## Playground and showcase

The landing page's editor accepts one self-contained TSX file; showcase entries load their full
source projects from the repository's `examples/`. Runtime imports are limited to
`@celesta/react`, `@celesta/math`, `@celesta/code`, and `react`. esbuild WASM compiles the
visitor's code in a Worker, Celesta's React reconciler produces each `Scene`, and `@celesta/web`
draws it on a canvas for preview and MP4 export (WebCodecs H.264/AAC via Mediabunny). The
preview is silent; constant-rate, constant-volume audio clips are mixed into the export. Media
added with **Add media** is referred to by file name. Browser text, video seeking, and fonts can
differ from the native renderer; use the desktop exporter for pixel-exact output.

Reel, Apex, Afterimage, Signal, Spectra, and 36 Days run in the browser. Prism and Feature Tour
need native PSD and voice preparation, and Versus needs generated benchmark videos; their pages
link to the source and setup instructions instead. Add a work to `site/showcase-catalog.ts`, add
its description to both copy files, and add `pages/showcase/<slug>.tsx` and `.meta.ts`
(`tests/content.test.ts` checks that every work has both). Keep `showcaseAssetDirectories` in
sync when an example needs new fonts or images.

## Verification

```sh
pnpm test
pnpm build
pnpm check
pnpm check:examples
pnpm deploy:check
```

`check:examples` uses the declarations staged by `packages/cli`'s build
(`dist/project-types/`); run `pnpm run build:runtime` from the repository root first.

Then run `pnpm preview:cloudflare` and check, at desktop and mobile widths in light and dark:
`/` redirects by `Accept-Language`; `/docs/export/` reaches `/en/docs/export/` in an English
browser; an unknown address such as `/en/nope/` shows the themed 404 with status 404; the
landing page's playground edits, plays, scrubs, and exports an MP4; a browser-ready and a
desktop-only showcase entry; documentation navigation, search with and without hits in both
languages, and the language switch on documentation and static pages.

The user guide follows the React workflow in the root README. Keep CLI flags aligned with
`crates/exporter/src/main.rs` and `crates/editor/src/cli.rs`, package import examples aligned
with each package's public `src/index.ts`, and run `pnpm check:examples` after changing any
example. In particular, VOICEVOX and AivisSpeech adapters are imported from `@celesta/voicevox`
and `@celesta/aivis-speech`, not `@celesta/react`.
