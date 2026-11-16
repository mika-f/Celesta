# aivis-speech

The `aivis-speech` plugin: AivisSpeech lip sync and timing, imported as `@celesta-plugin/aivis-speech`. It is
component-only (no process) and is made from the workspace package
[`@celesta.studio/aivis-speech`](../../packages/aivis-speech/), whose `src/index.ts` is the
plugin's module. `scripts/plugins/build.mts` bundles that module into one
ES module, `react` and `@celesta.studio/*` left as imports the runtime
serves, and `plugin-release.yml` publishes it to registry.celesta.studio as
the target `any`, at Celesta's version.

The `@celesta.studio/aivis-speech` import still works: Celesta keeps the package
bundled and warns, naming this plugin. See
[`docs/handoff/2026-11-16-package-plugins.md`](../../docs/handoff/2026-11-16-package-plugins.md).
