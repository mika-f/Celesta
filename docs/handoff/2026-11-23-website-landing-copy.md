# Website landing page copy refresh

## Goal

Update outdated feature descriptions on the landing page (`packages/website`) so the four "At a glance" summary cards and related sections reflect Celesta's current capabilities while staying concise and readable as a product landing page.

## Changes

- **At a glance (4 cards)** ([`packages/website/site/copy/en.ts`](../../packages/website/site/copy/en.ts), [`packages/website/site/copy/ja.ts`](../../packages/website/site/copy/ja.ts), [`packages/website/pages/index.tsx`](../../packages/website/pages/index.tsx)):
  - `01 Write`: updated to cover React + TypeScript with either a text editor or AI coding agents.
  - `02 Preview`: added direct canvas editing with source write-back in the desktop app alongside browser preview and synced audio playback.
  - `03 Export`: replaced "H.264 and AAC" / MP4-only wording with MP4, transparent MOV/WebM, animated GIF, and still frames from the app, CLI, or browser.
  - `04 Extend` (replaced `04 License`, which remains in the hero eyebrow and footer): highlights the open-source Rust engine's extensibility via plugins, CLI automation, and MCP tools.
- **Related landing page copy** (kept light and concise):
  - Features card 2 (`Preview`): mentions writing canvas edits back to source.
  - Features card 3 (`Export`): expands from MP4-only to MP4, transparent MOV/WebM, GIF, and still frames.
  - Get started: updates bundled dependency wording to "React runtime, Node.js, and media plugins" (matching `README.md`) and mentions creating a project from a template alongside starting from a single `.tsx` file.

## Verification

- Passed `tsc -p packages/website/tests`, `tsc -p packages/website/tests/tsconfig.worker.json`, `vitest run` (8 test files, 52 tests including `tests/copy.test.ts` key parity), and `makit check`.
- Visually checked `/ja/` and `/en/` at `1440x900` in dark mode to verify the 4-column balance and line wrapping.
