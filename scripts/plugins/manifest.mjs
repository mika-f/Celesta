// A plugin directory's manifest (celesta-plugin.json), from the plugin's
// plugin.json and what the build made. A plugin with a process is versioned by
// its Cargo package, so plugin.json does not repeat (and drift from) that
// version; a plugin without one says its version in plugin.json.

/**
 * plugin.json: what a plugin's source has.
 *
 * @typedef {object} PluginSource
 * @property {string} id
 * @property {string} [version] Only for a plugin without `cargoPackage`.
 * @property {string} [cargoPackage] The Cargo package, and binary, of the plugin's process.
 * @property {string} module The component module's entry, relative to the plugin's folder.
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
 * @property {Record<string, string>} [executable]
 * @property {'js/index.js'} module
 * @property {'js/index.d.ts'} types
 * @property {object[]} [components]
 */

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
  return {
    id: source.id,
    version: pluginVersion(source, cargoVersion, sourcePath),
    protocol: 1,
    ...(executable ? { executable } : {}),
    module: 'js/index.js',
    types: 'js/index.d.ts',
    ...(source.components ? { components: source.components } : {}),
  };
}

/**
 * @param {PluginSource} source
 * @param {string | undefined} cargoVersion
 * @param {string} sourcePath
 * @returns {string}
 */
function pluginVersion({ version, cargoPackage }, cargoVersion, sourcePath) {
  if (cargoPackage !== undefined) {
    if (version !== undefined) {
      throw new Error(`${sourcePath}: version comes from the Cargo package ${cargoPackage}; remove it`);
    }
    if (cargoVersion === undefined) {
      throw new Error(`${sourcePath}: Cargo gave no version for the package ${cargoPackage}`);
    }
    return cargoVersion;
  }
  if (version === undefined) {
    throw new Error(`${sourcePath}: version is required for a plugin without a cargoPackage`);
  }
  return version;
}
