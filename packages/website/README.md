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
  `src/syntax.ts` highlights code with [twinkleplop](https://twinkleplop.pngwn.at)
  (TSX, JSON, and shell); the playground editor uses it too, layering a
  transparent textarea over the highlighted source.
- `src/examples/`: complete documentation examples (`first-scene.tsx`,
  `dialogue.tsx`, `lip-sync.tsx`, `media.tsx`, `dialogue.celesta.json`). The
  docs import them as raw text. They are excluded from the site's own
  type-check and checked against the real `@celesta/react` instead.
  The JSON chapter imports `../../examples/editor-demo.celesta.json` from the
  repository so its example stays in sync. Build with the repository present.
- `src/demo/title-scene.tsx`: editable starting composition.
- `src/Playground.tsx`: TSX editor and file/media import. `@celesta/web` provides
  frame preview and MP4 export. esbuild WASM compiles the visitor's code in a
  dedicated Worker, and Celesta's real React reconciler/evaluator produces
  each `Scene`. The browser canvas renderer draws that Scene for both preview
  and export. Mediabunny uses WebCodecs for H.264/AAC and MP4 muxing.
- `src/style.css`: design tokens, responsive layouts, and reduced-motion styles.
- `src/Brand.tsx` and `public/favicon.svg`: the website's crescent mark (a "C"
  with one star). It is separate from the app logos in `packages/logos`.

The web editor accepts one self-contained TSX file. Runtime imports are limited
to `@celesta/react`, `@celesta/math`, and `react`; `prepare()`, companion JSON projects, PSD
portraits, and other npm imports still require the desktop/CLI workflow. Add
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

Check the page at desktop and mobile widths: navigation, TSX changes,
playback/pause/scrubbing, source opening/copying/downloading, media import,
canceling, and MP4 download. After changing the scene, export it and inspect
the MP4 codecs, frame count, and an actual decoded frame. Test
`pnpm preview:cloudflare` for static asset serving and missing-page responses.

For documentation changes, verify topic search (including no results), OS
selection, code copying, mobile contents navigation, and direct visits to
`/docs/#export` using both Vite and local Wrangler. Keep guide instructions in
sync with the root README and the React API, and run `pnpm check:examples`
after changing any example.
