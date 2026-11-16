/** The scope the Celesta packages had before `@celesta.studio`. */
const LEGACY_SCOPE = '@celesta/';

/**
 * @internal `specifier` with the legacy `@celesta/` scope renamed to
 * `@celesta.studio/`, so entries written before the packages moved still
 * import the runtime's; any other specifier unchanged.
 */
export function currentPackageName(specifier: string): string {
  return specifier.startsWith(LEGACY_SCOPE) ? `@celesta.studio/${specifier.slice(LEGACY_SCOPE.length)}` : specifier;
}

/**
 * The packages that became first-party registry plugins, by package name,
 * with the plugin's id. Celesta still serves the packages, so entries that
 * import them keep working, and warns that they moved.
 */
const MOVED_TO_PLUGINS: ReadonlyMap<string, string> = new Map([
  ['@celesta.studio/aivis-speech', 'aivis-speech'],
  ['@celesta.studio/code', 'code'],
  ['@celesta.studio/lottie', 'lottie'],
  ['@celesta.studio/voicevox', 'voicevox'],
]);

/**
 * @internal The id of the plugin the package `specifier` names (by its
 * current name, see {@link currentPackageName}) became, such as `code` for
 * `@celesta.studio/code`; undefined for any other specifier.
 */
export function movedToPlugin(specifier: string): string | undefined {
  return MOVED_TO_PLUGINS.get(specifier);
}

/**
 * @internal The package the first-party plugin import `specifier`
 * (`@celesta-plugin/code`) was made from (`@celesta.studio/code`), for a
 * runtime that serves the packages in place of installed plugins (the web
 * editor); undefined for any other specifier.
 */
export function packageOfPlugin(specifier: string): string | undefined {
  for (const [name, id] of MOVED_TO_PLUGINS) {
    if (specifier === `@celesta-plugin/${id}`) return name;
  }
  return undefined;
}
