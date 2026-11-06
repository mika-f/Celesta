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
