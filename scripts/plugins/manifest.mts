// A plugin directory's manifest (celesta-plugin.json), from the plugin's
// plugin.json and what the build made. plugin.json gives the version. A
// plugin with a process gives the same version in its Cargo package; the two
// are bumped together, and a plugin whose versions differ is refused.

/** plugin.json: what a plugin's source has. */
export interface PluginSource {
  id: string;
  /** Required; with `cargoPackage`, that package's version. */
  version?: string;
  /** The Cargo package, and binary, of the plugin's process. */
  cargoPackage?: string;
  /** The component module's entry, relative to the plugin's folder. */
  module?: string;
  /** What the plugin is to Celesta (`content` when absent, `codec`). */
  roles?: string[];
  /** Files, relative to the plugin's folder, copied by name into licenses/. */
  licenses?: string[];
  components?: object[];
}

/** What the build made. */
export interface Built {
  /** The version of `cargoPackage`, from Cargo. */
  cargoVersion?: string;
  /** Each platform's process, as `<os>-<arch>` to a path. */
  executable?: Record<string, string>;
}

/** celesta-plugin.json. */
export interface PluginManifest {
  id: string;
  version: string;
  protocol: 1;
  roles?: string[];
  executable?: Record<string, string>;
  module?: 'js/index.js';
  types?: 'js/index.d.ts';
  components?: object[];
}

/** The keys plugin.json may have, as PluginSource lists them. */
const PLUGIN_SOURCE_KEYS = ['id', 'version', 'cargoPackage', 'module', 'roles', 'licenses', 'components'];

/**
 * Checks that the plugin `source` (read from `sourcePath`, which errors name)
 * has only the keys PluginSource lists, so a misspelt one is not skipped
 * without a word, and has something to build: a module, a Cargo package, or
 * both, and a module for any components, since Celesta skips a plugin listing
 * components it cannot import.
 */
export function checkPluginSource(source: PluginSource, sourcePath: string): void {
  const unknown = Object.keys(source).filter((key) => !PLUGIN_SOURCE_KEYS.includes(key));
  if (unknown.length > 0) {
    throw new Error(
      `${sourcePath}: unknown key${unknown.length === 1 ? '' : 's'} ${unknown.join(', ')}; plugin.json may have ${PLUGIN_SOURCE_KEYS.join(', ')}`,
    );
  }
  if (source.module === undefined && source.components !== undefined) {
    throw new Error(`${sourcePath}: components need a module to import them from`);
  }
  if (source.module === undefined && source.cargoPackage === undefined) {
    throw new Error(`${sourcePath}: it has neither a module nor a cargoPackage to build`);
  }
}

/**
 * The manifest of the plugin `source` (read from `sourcePath`, which errors
 * name) once `built`.
 */
export function pluginManifest(source: PluginSource, { cargoVersion, executable }: Built, sourcePath: string): PluginManifest {
  checkPluginSource(source, sourcePath);
  return {
    id: source.id,
    version: pluginVersion(source, cargoVersion, sourcePath),
    protocol: 1,
    ...(source.roles ? { roles: source.roles } : {}),
    ...(executable ? { executable } : {}),
    ...(source.module !== undefined ? { module: 'js/index.js', types: 'js/index.d.ts' } : {}),
    ...(source.components ? { components: source.components } : {}),
  };
}

/**
 * plugin.json's version, once it is checked: it must be there, and with a
 * `cargoPackage` it must be `cargoVersion`, that package's version.
 */
export function pluginVersion(
  { version, cargoPackage }: Pick<PluginSource, 'version' | 'cargoPackage'>,
  cargoVersion: string | undefined,
  sourcePath: string,
): string {
  if (version === undefined) {
    throw new Error(`${sourcePath}: version is required`);
  }
  if (cargoPackage !== undefined) {
    if (cargoVersion === undefined) {
      throw new Error(`${sourcePath}: Cargo gave no version for the package ${cargoPackage}`);
    }
    if (cargoVersion !== version) {
      throw new Error(`${sourcePath}: version ${version} differs from the Cargo package ${cargoPackage}'s ${cargoVersion}`);
    }
  }
  return version;
}
