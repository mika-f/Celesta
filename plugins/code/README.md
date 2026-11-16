# code

The `code` plugin: syntax-highlighted code components, imported as `@celesta-plugin/code`. It is
component-only (no process) and is made from the workspace package
[`@celesta.studio/code`](../../packages/code/), whose `src/index.ts` is the
plugin's module. `scripts/plugins/build.mts` bundles that module into one
ES module, `react` and `@celesta.studio/*` left as imports the runtime
serves, and `plugin-release.yml` publishes it to registry.celesta.studio as
the target `any`, at Celesta's version.

The `@celesta.studio/code` import still works: Celesta keeps the package
bundled and warns, naming this plugin. See
[`docs/handoff/2026-11-16-package-plugins.md`](../../docs/handoff/2026-11-16-package-plugins.md).
