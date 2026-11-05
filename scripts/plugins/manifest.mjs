// A plugin directory's manifest (celesta-plugin.json), from the plugin's
// plugin.json and what the build made. plugin.json gives the version. A
// plugin with a process gives the same version in its Cargo package; the two
// are bumped together, and a plugin whose versions differ is refused.

/**
 * plugin.json: what a plugin's source has.
 *
 * @typedef {object} PluginSource
 * @property {string} id
 * @property {string} [version] Required; with `cargoPackage`, that package's version.
 * @property {string} [cargoPackage] The Cargo package, and binary, of the plugin's process.
 * @property {string} [module] The component module's entry, relative to the plugin's folder.
 * @property {string[]} [roles] What the plugin is to Celesta (`content` when absent, `codec`).
 * @property {string[]} [licenses] Files, relative to the plugin's folder, copied by name into licenses/.
 * @property {object[]} [components]
 */

/**
 * What the build made.
 *
 * @typedef {object} Built
 * @property {string} [cargoVersion] The version of `cargoPackage`, from Cargo.
 * @property {Record<string, string>} [executable] Each platform's process, as `<os>-<arch>` to a path.
 */

/**
 * celesta-plugin.json.
 *
 * @typedef {object} PluginManifest
 * @property {string} id
 * @property {string} version
 * @property {1} protocol
 * @property {string[]} [roles]
 * @property {Record<string, string>} [executable]
 * @property {'js/index.js'} [module]
 * @property {'js/index.d.ts'} [types]
 * @property {object[]} [components]
 */

/**
 * Checks that the plugin `source` (read from `sourcePath`, which errors name)
 * has something to build: a module, a Cargo package, or both, and a module
 * for any components, since Celesta skips a plugin listing components it
 * cannot import.
 *
 * @param {PluginSource} source
 * @param {string} sourcePath
 */
export function checkPluginSource(source, sourcePath) {
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
 *
 * @param {PluginSource} source
 * @param {Built} built
 * @param {string} sourcePath
 * @returns {PluginManifest}
 */
export function pluginManifest(source, { cargoVersion, executable }, sourcePath) {
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
 *
 * @param {Pick<PluginSource, 'version' | 'cargoPackage'>} source
 * @param {string | undefined} cargoVersion
 * @param {string} sourcePath
 * @returns {string}
 */
export function pluginVersion({ version, cargoPackage }, cargoVersion, sourcePath) {
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
