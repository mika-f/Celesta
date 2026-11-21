# score

The `score` plugin: sound effects, drums, and simple music written as data
and synthesized deterministically, imported as `@celesta-plugin/score`. It is
component-only (no process) and is made from the workspace package
[`@celesta.studio/score`](../../packages/score/), whose `src/index.ts` is the
plugin's module. `scripts/plugins/build.mts` bundles that module into one
ES module, `react` and `@celesta.studio/*` left as imports the runtime
serves, and `plugin-release.yml` publishes it to registry.celesta.studio as
the target `any`, at Celesta's version.

Unlike `code`, `lottie`, `voicevox`, and `aivis-speech`, it was never a
bundled `@celesta.studio/*` package, so there is no old import to keep. See
[`docs/handoff/2026-11-22-score-synth.md`](../../docs/handoff/2026-11-22-score-synth.md).
