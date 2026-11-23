# Website Landing Page "Good for" Showcase Cards

- Date: 2026-11-23
- Scope: `packages/website`, `examples/assets/dialogue-demo/poster.jpg`

## Goal

Redesign the landing page "Good for" / "活用例" section (`#ideas-heading`) so visitors can immediately see what kinds of videos Celesta builds, using concrete 16:9 example visuals and concise descriptions, while keeping the section compact and aligned with the Noir theme.

## Changes

- **Example poster asset (`examples/assets/dialogue-demo/poster.jpg`, `packages/website/site/showcase-catalog.ts`, `packages/website/playground/vite.config.ts`)**:
  - Added a 16:9 (1280x720) Yukkuri-style dialogue explainer poster featuring Celesta's original MIT/Apache-2.0 portrait characters (`shizuku` and `komugi` from `examples/assets/dialogue-demo/portraits/`), a `planDialogue` / `DialogueSeries` code slide, a timeline bar, and outlined subtitles.
  - Added `showcaseExtraImages` to `site/showcase-catalog.ts` and copied those extra poster images into `public/showcase/` during the Vite island build in `playground/vite.config.ts`.
  - Added an existence test in `packages/website/tests/content.test.ts` so every `showcaseExtraImages` source file is verified by Vitest.
- **Landing page cards & links (`packages/website/pages/index.tsx`, `packages/website/site/copy/{en,ja}.ts`)**:
  - Replaced the plain 4-column text list with 4 visual cards pairing 16:9 stills (`feature-tour.jpg`, `dialogue-demo.jpg`, `36-days.jpg`, `afterimage.jpg`), short descriptions, and uniform links to the corresponding documentation guides:
    - `01` Explainers & releases -> `/docs/layout/`
    - `02` Dialogue & lip sync -> `/docs/dialogue/`
    - `03` Data-driven video -> `/docs/data/`
    - `04` Generative motion -> `/docs/animation/`
  - Added a section footer link (`.lp-ideas-footer`) pointing to `/${locale}/showcase/` so all cards have a consistent role (guides) while the footer leads to the full showcase.
- **Compact responsive layout (`packages/website/pages/_components/landing.css`)**:
  - Styled `.lp-ideas-grid` as 4 columns on desktop (`>= 1024px`) and 2 columns below `1024px` so the section fits comfortably within a single desktop viewport and stays compact on mobile (`390px`).
  - Kept Noir flat surfaces, `--nr-radius: 0`, 1px hairline borders, unified guide-link hover accent, and `prefers-reduced-motion` support for thumbnail hover transitions.

## Verification

- `tsc -p packages/website/tests` — passed.
- `tsc -p packages/website/tests/tsconfig.worker.json` — passed.
- `vitest run --root packages/website` — passed (8 test files, 52 tests, including `tests/copy.test.ts` key parity and `tests/content.test.ts` extra showcase image existence check).
- `makit check` — passed with 0 errors.
- Visual browser check on `http://localhost:3000/ja/` and `/en/`:
  - `1440x900` dark and light modes: 4-column single row, crisp 16:9 posters (`HTTP 200`), balanced card heights, verified light/dark contrast.
  - `390x844` dark mode: 2-column (2x2) grid with zero horizontal overflow or clipping.
