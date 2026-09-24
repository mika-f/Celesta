# Celesta website

An English product website built with Vite, React, TypeScript, and Tailwind CSS 4.
It is an independent pnpm package, like `packages/logos` and `packages/react`.

## Develop

Use Node.js 22.12+ (Node.js 24 LTS recommended) and pnpm 12.4.2.

```sh
cd packages/website
pnpm install --frozen-lockfile
pnpm dev
```

Vite prints the local URL. `pnpm build` type-checks the site and writes static
files to `dist/`. `pnpm preview` serves that production build.

## Deploy to Cloudflare Workers

The site uses [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/).
No API server, bindings, database, or runtime secrets are required.
`wrangler.jsonc` sets the Worker name to `celesta-website` and serves `dist/`.
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
Vite builds separate HTML entries for `/` and `/docs/`, so documentation deep
links such as `/docs/#export` work on direct visits without an SPA fallback.
The included `404.html` handles unknown routes; `public/_headers` defines response
headers and immutable caching for Vite's hashed assets.

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

## Content and behavior

- `src/App.tsx`: product copy, navigation, documentation and repository links.
- `src/links.ts` and `src/Download.tsx`: repository and download links, and the
  platform-aware download buttons.
- `/docs/`: the on-site English user guide, built from `docs/index.html`.
  `src/Docs.tsx` provides the responsive table of contents and topic search;
  `src/docs-content.tsx` contains setup, preview, React, animation, layout,
  media, JSON timeline, dialogue and lip sync, data, export, API, example, and
  troubleshooting chapters.
  `src/DocCode.tsx` supplies accessible copy controls for code examples.
- `src/examples/`: complete documentation examples (`first-scene.tsx`,
  `dialogue.tsx`, `lip-sync.tsx`, `media.tsx`, `dialogue.celesta.json`). The
  docs import them as raw text. They are excluded from the site's own
  type-check and checked against the real `@celesta/react` instead.
  The JSON chapter imports `../../examples/editor-demo.celesta.json` from the
  repository so its example stays in sync. Build with the repository present.
- `src/demo/title-scene.tsx`: the playground composition, a real Celesta scene.
  The playground shows this file's source with the visitor's title and palette
  filled in, and offers it as `my-first-scene.tsx`.
- `src/demo/celesta-browser.ts`: a small browser implementation of the part of
  `@celesta/react` the scene uses. Vite and `tsconfig.json` alias
  `@celesta/react` to it, and it draws the scene on a canvas with the native
  renderer's rules (transform order, anchors, per-layer opacity, inner rect
  strokes, and text trimmed to its visible glyphs). The preview therefore
  matches what Celesta exports. If the scene uses another component, add it
  here first.
- `src/Playground.tsx`: the interactive preview with an editable title, three
  palettes, frame scrubbing, explicit playback, and the scrollable source.
- `src/style.css`: design tokens, responsive layouts, and reduced-motion styles.
- `public/favicon.svg`: the existing Celesta Starlight logo from `packages/logos`.

The browser demo does not load Celesta's native renderer or export MP4. Playback
is opt-in and stops after one five-second pass. Press Play to restart; dragging
the frame slider pauses playback. To render the downloaded `.tsx`, open it in
the installed Celesta app. Source-build users should first follow the developer
setup chapter. The scene uses the Georgia font, which ships with macOS and
Windows; other systems fall back to a sans-serif font in both the browser and
Celesta.

The website uses packaged macOS and Windows downloads as the primary onboarding
flow. The source-build workflow is a separate developer chapter. Keep
installation copy aligned with the packaging scripts and published release
assets. There are no external fonts, analytics, or media requests. Public
links use the repository's configured GitHub origin.

## Verification

```sh
pnpm build
pnpm check:examples
pnpm deploy:check
```

`check:examples` type-checks the playground scene and every file in
`src/examples/` against the declarations staged by `packages/react`'s
`pnpm run build` (`dist/project-types/`). Build that package first.

Check the page at desktop and mobile widths: navigation, palette selection,
title updates, playback/pause/scrubbing, source copying and download. After changing
the scene, open the downloaded file in Celesta (or export it with
`celesta-exporter --react`) and compare a frame with the browser preview. Test
`pnpm preview:cloudflare` for static asset serving and missing-page responses.

For documentation changes, verify topic search (including no results), OS
selection, code copying, mobile contents navigation, and direct visits to
`/docs/#export` using both Vite and local Wrangler. Keep guide instructions in
sync with the root README and the React API, and run `pnpm check:examples`
after changing any example.
