// A plugin directory's manifest (celesta-plugin.json), from the plugin's
// plugin.json and what the build made. plugin.json gives the version. A
// plugin with a process gives the same version in its Cargo package; the two
// are bumped together, and a plugin whose versions differ is refused. The
// manifest's `celesta` range comes from the Celesta the plugin was built
// against, so it does not drift by hand.

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
  /** What a media plugin offers: its decoders, copied as they are. */
  media?: object;
}

/** What the build made. */
export interface Built {
  /**
   * The version of the Celesta it was built against: its crates' and its
   * runtime packages' (the root Cargo.toml's `[workspace.package]` version).
   */
  celestaVersion: string;
  /** The version of `cargoPackage`, from Cargo. */
  cargoVersion?: string;
  /** Each platform's process, as `<os>-<arch>` to a path. */
  executable?: Record<string, string>;
}

/** celesta-plugin.json. */
export interface PluginManifest {
  id: string;
  version: string;
  /** 2 for a plugin with the `media` role, 1 for the rest. */
  protocol: 1 | 2;
  /** The Celesta versions it works with: {@link celestaRange} of the one it was built against. */
  celesta: string;
  roles?: string[];
  executable?: Record<string, string>;
  module?: 'js/index.js';
  types?: 'js/index.d.ts';
  components?: object[];
  media?: object;
}

/** The keys plugin.json may have, as PluginSource lists them. */
const PLUGIN_SOURCE_KEYS = ['id', 'version', 'cargoPackage', 'module', 'roles', 'licenses', 'components', 'media'];

/**
 * Celesta's rule for a plugin id (`is_valid_id` in
 * `crates/plugin-host/src/manifest.rs`): lowercase ASCII letters, digits, `-`,
 * `_`, and `.`, starting with a letter or digit. The build names the plugin's
 * folder under `dist/` after it, so the rule also keeps that folder there.
 */
const PLUGIN_ID = /^[a-z0-9][a-z0-9._-]*$/;

/**
 * Checks that the plugin `source` (read from `sourcePath`, which errors name)
 * has only the keys PluginSource lists, so a misspelt one is not skipped
 * without a word, has an `id` Celesta accepts (see {@link PLUGIN_ID}), has
 * `components`, if any, as a list of objects, and has something to build: a
 * module, a Cargo package, or both, and a module for any components, since
 * Celesta skips a plugin listing components it cannot import (an empty
 * `components` list, which Celesta accepts, needs none).
 */
export function checkPluginSource(source: PluginSource, sourcePath: string): void {
  const unknown = Object.keys(source).filter((key) => !PLUGIN_SOURCE_KEYS.includes(key));
  if (unknown.length > 0) {
    throw new Error(
      `${sourcePath}: unknown key${unknown.length === 1 ? '' : 's'} ${unknown.join(', ')}; plugin.json may have ${PLUGIN_SOURCE_KEYS.join(', ')}`,
    );
  }
  // plugin.json is read as JSON, so `id` and `components` may be anything at all.
  const id: unknown = source.id;
  if (typeof id !== 'string' || id === '') {
    throw new Error(`${sourcePath}: id must be a non-empty string`);
  }
  if (!PLUGIN_ID.test(id)) {
    throw new Error(
      `${sourcePath}: id ${JSON.stringify(id)} may have only lowercase ASCII letters, digits, -, _, and ., starting with a letter or digit`,
    );
  }
  if (id === 'builtin') {
    throw new Error(`${sourcePath}: the id "builtin" is reserved for the components Celesta has built in`);
  }
  if (source.roles?.includes('content') && source.roles.includes('media')) {
    throw new Error(`${sourcePath}: a plugin is either content or media`);
  }
  const components: unknown = source.components;
  if (
    components !== undefined &&
    !(Array.isArray(components) && components.every((entry) => typeof entry === 'object' && entry !== null && !Array.isArray(entry)))
  ) {
    throw new Error(`${sourcePath}: components must be a list of objects`);
  }
  const noComponents = source.components === undefined || source.components.length === 0;
  if (source.module === undefined && !noComponents) {
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
export function pluginManifest(
  source: PluginSource,
  { celestaVersion, cargoVersion, executable }: Built,
  sourcePath: string,
): PluginManifest {
  checkPluginSource(source, sourcePath);
  return {
    id: source.id,
    version: pluginVersion(source, cargoVersion, sourcePath),
    protocol: source.roles?.includes('media') ? 2 : 1,
    celesta: celestaRange(celestaVersion),
    ...(source.roles ? { roles: source.roles } : {}),
    ...(executable ? { executable } : {}),
    ...(source.module !== undefined ? { module: 'js/index.js', types: 'js/index.d.ts' } : {}),
    ...(source.components ? { components: source.components } : {}),
    ...(source.media ? { media: source.media } : {}),
  };
}

/**
 * plugin.json's version, once it is checked: it must be a non-empty string,
 * and with a `cargoPackage` it must be `cargoVersion`, that package's version.
 */
export function pluginVersion(
  { version, cargoPackage }: Pick<PluginSource, 'version' | 'cargoPackage'>,
  cargoVersion: string | undefined,
  sourcePath: string,
): string {
  if (version === undefined) {
    throw new Error(`${sourcePath}: version is required`);
  }
  // plugin.json is read as JSON, so `version` may be anything at all.
  const given: unknown = version;
  if (typeof given !== 'string' || given === '') {
    throw new Error(`${sourcePath}: version must be a non-empty string`);
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

/** A semver version: its major, minor, and patch, its pre-release, and its build metadata. */
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$/;

/**
 * Whether `identifiers` (a pre-release, or with `numeric` false build
 * metadata) are dot-separated identifiers as semver allows them: none empty,
 * and with `numeric`, none all digits with a leading zero.
 */
function validIdentifiers(identifiers: string, numeric: boolean): boolean {
  return identifiers
    .split('.')
    .every((identifier) => identifier !== '' && !(numeric && /^0\d+$/.test(identifier)));
}

/**
 * The `celesta` range of a plugin built against Celesta `version`: that
 * version (without build metadata) up to the next release that may break it,
 * as Cargo's `^` reads (the next patch below 0.1, the next minor below 1.0,
 * the next major from it), such as `>=0.9.0, <0.10.0`. Celesta's
 * `celesta_range_for` (`crates/plugin-host/src/manifest.rs`) suggests the
 * same.
 */
export function celestaRange(version: string): string {
  const match = SEMVER.exec(version);
  const [, major, minor, patch, pre, build] = match ?? [];
  if (
    match === null ||
    (pre !== undefined && !validIdentifiers(pre, true)) ||
    (build !== undefined && !validIdentifiers(build, false))
  ) {
    throw new Error(`Celesta's version ${JSON.stringify(version)} is not a semver version`);
  }
  const next =
    major === '0' && minor === '0'
      ? `0.0.${Number(patch) + 1}`
      : major === '0'
        ? `0.${Number(minor) + 1}.0`
        : `${Number(major) + 1}.0.0`;
  return `>=${major}.${minor}.${patch}${pre === undefined ? '' : `-${pre}`}, <${next}`;
}
