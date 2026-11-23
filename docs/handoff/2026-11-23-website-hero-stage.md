# Website hero live stage and interactive timeline

## Goal

Fill the empty upper-right space of the landing page hero (`packages/website`) and turn the decorative timeline bar underneath into a synchronized, interactive preview while keeping the Noir design system intact (flat fills, square corners, 1 px hairlines, one electric blue, light/dark parity).

## Specification & Implementation

- **Deterministic 30 fps canvas choreography** ([`packages/website/pages/_components/hero-stage-draw.ts`](../../packages/website/pages/_components/hero-stage-draw.ts)):
  - Pure `drawHeroStage(ctx, frame, palette)` renderer over a 10-second loop (`HERO_TOTAL_FRAMES = 300` at 30 fps) partitioned into three contiguous clips:
    1. `story.tsx` (`0..104`): kinetic typography reveal with a Celesta-style layer selection box, live coordinate tag, and `interpolate(...)` readout.
    2. `atmosphere` (`105..204`): 16-layer phased rotating wireframe ribbon with orbiting coordinate nodes that collapses into the center baseline at clip end.
    3. `voice.wav` (`205..299`): 36 deterministic audio waveform bars, synchronized vertical scan playhead, and stereo L/R level meters.
  - Shared `formatHeroTimecode`, `clipAtFrame`, `frameToRatio`, and `ratioToFrame` keep the stage, active clip highlight, playhead position, and timecode (`00:00:00:00`–`00:00:09:29`) locked to one clock.
- **Interactive stage & timeline component** ([`packages/website/pages/_components/HeroShowcase.tsx`](../../packages/website/pages/_components/HeroShowcase.tsx)):
  - Replaces `Timecode.tsx` (removed). Renders the 16:9 stage and the interactive timeline rail as direct children of `.lp-hero-grid`.
  - Supports pointer click/drag scrubbing (`setPointerCapture`, pointer hover indicator, no stray pointer focus ring), play/pause toggle, keyboard scrubbing (`ArrowLeft`/`ArrowRight`, `Shift` for 10-frame jumps, `Home`/`End`, `Space`/`Enter`), `prefers-reduced-motion` (static representative frame `52`), and live light/dark palette updates via `MutationObserver` + `prefers-color-scheme`.
- **Responsive hero layout** ([`packages/website/pages/index.tsx`](../../packages/website/pages/index.tsx), [`packages/website/pages/_components/landing.css`](../../packages/website/pages/_components/landing.css)):
  - Keeps the 2-column upper-right stage layout down to `640px` (`clamp(230px, min(37vw, 54dvh), 440px)`) so narrow or short desktop windows use the upper-right space without pushing content off-screen. Uses `@container (max-width: 310px)` to hide `30 FPS ·` in the stage header when compact.
  - Below `860px`, `.lp-hero-lower` stacks the lead paragraph and CTAs while keeping the stage in the upper-right down to `640px`.
  - Below `640px` (phones), stacks the stage directly above the timeline and caps `.lp-stage-viewport` height to `min(52vw, 200px, 28dvh)` with `object-fit: contain` so the 16:9 canvas fits cleanly without ballooning vertically.
  - Added `white-space: nowrap` to `.lp-alternatives a` to prevent orphaned external-link arrows on narrow screens.
- **Localized labels** ([`packages/website/site/copy/en.ts`](../../packages/website/site/copy/en.ts), [`packages/website/site/copy/ja.ts`](../../packages/website/site/copy/ja.ts)):
  - Added symmetric `home.heroTimeline` accessibility and transport labels in English and Japanese.

## Verification

- Unit tests: added [`packages/website/tests/hero-stage.test.ts`](../../packages/website/tests/hero-stage.test.ts) covering timecode wrapping, clip contiguity, and ratio/frame conversions.
- Passed `wrangler types`, `tsc -p packages/website/tests`, `tsc -p packages/website/tests/tsconfig.worker.json`, `vitest run` (9 test files, 55 tests), and `makit check`.
- Visual & interactive verification in browser (`http://localhost:3000/ja/` and `/en/`) across dark/light themes, all three clips, and viewports `1440x900`, `900x680`, `768x640`, `640x640`, `560x720`, and `390x844`.
