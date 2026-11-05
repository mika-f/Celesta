// A Cargo.lock's packages, and the local ones a package reaches. The lockfile
// already merges normal, build, and dev dependencies and holds `[patch]` path
// packages too, so following it finds every local package a build reads.

/**
 * @typedef {{ name: string, version: string, source: string | null, dependencies: string[] }} LockPackage
 * `source` is `null` for a local package (a workspace member or a path dependency).
 */

const QUOTED = /^"((?:[^"\\]|\\.)*)"/;

/**
 * The `[[package]]` tables of a Cargo.lock, read line by line (Cargo writes
 * each key on its own line and each dependency on its own line).
 *
 * @param {string} text The lockfile's contents, with `\n` or `\r\n` line endings.
 * @param {string} path The lockfile's path, for error messages.
 * @returns {LockPackage[]}
 */
export function parseCargoLock(text, path) {
  /** @type {LockPackage[]} */
  const packages = [];
  /** @type {{ name?: string, version?: string, source: string | null, dependencies: string[] } | undefined} */
  let current;
  let inDependencies = false;

  const finish = () => {
    if (current === undefined) return;
    const { name, version, source, dependencies } = current;
    if (name === undefined || version === undefined) {
      const known = name !== undefined ? ` (name ${name})` : version !== undefined ? ` (version ${version})` : '';
      throw new Error(`${path}: a [[package]] has no name or version${known}`);
    }
    packages.push({ name, version, source, dependencies });
    current = undefined;
  };

  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (inDependencies) {
      if (line.startsWith(']')) inDependencies = false;
      else {
        const entry = QUOTED.exec(line);
        if (entry) current?.dependencies.push(entry[1]);
      }
      continue;
    }
    if (line.startsWith('[')) {
      finish();
      if (line === '[[package]]') current = { source: null, dependencies: [] };
      continue;
    }
    if (current === undefined) continue;
    const pair = /^([A-Za-z0-9_-]+)\s*=\s*(.*)$/.exec(line);
    if (!pair) continue;
    const [, key, value] = pair;
    if (key === 'dependencies') {
      const inline = /^\[(.*)\]$/.exec(value);
      if (inline) {
        for (const item of inline[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)) current.dependencies.push(item[1]);
      } else inDependencies = true;
      continue;
    }
    const quoted = QUOTED.exec(value);
    if (!quoted) continue;
    if (key === 'name') current.name = quoted[1];
    else if (key === 'version') current.version = quoted[1];
    else if (key === 'source') current.source = quoted[1];
  }
  finish();
  return packages;
}

/**
 * The sorted names of the local packages (`source === null`) reachable from
 * every package named `name`, those included, following every dependency,
 * registry and git packages' too (a registry package can depend on a `[patch]`
 * path package).
 *
 * @param {LockPackage[]} packages A lockfile's packages, from {@link parseCargoLock}.
 * @param {string} name The package to start from.
 * @param {string} path The lockfile's path, for error messages.
 * @returns {string[]}
 */
export function localPackages(packages, name, path) {
  const start = packages.filter((pkg) => pkg.name === name);
  if (start.length === 0) throw new Error(`${path}: no package ${name}`);

  /** @type {Set<LockPackage>} */
  const seen = new Set(start);
  const queue = [...start];
  for (let pkg = queue.shift(); pkg !== undefined; pkg = queue.shift()) {
    for (const entry of pkg.dependencies) {
      const dependency = resolve(packages, entry, path);
      if (seen.has(dependency)) continue;
      seen.add(dependency);
      queue.push(dependency);
    }
  }
  const local = new Set([...seen].filter(({ source }) => source === null).map((pkg) => pkg.name));
  return [...local].sort();
}

/**
 * The package a dependency entry names: `"name"`, `"name version"`, or
 * `"name version (source)"`, matched on name, then version, then source.
 * An entry's source leaves out the `#<revision>` a git package's `source`
 * ends with. Cargo writes no source for a local package even when a registry
 * or git package has the same name and version, so an entry without a source
 * that matches several packages names the one local package among them.
 *
 * @param {LockPackage[]} packages
 * @param {string} entry
 * @param {string} path
 * @returns {LockPackage}
 */
function resolve(packages, entry, path) {
  const match = /^(\S+)(?: (\S+)(?: \((.*)\))?)?$/.exec(entry);
  let candidates = match
    ? packages.filter(
        ({ name, version, source }) =>
          name === match[1] &&
          (match[2] === undefined || version === match[2]) &&
          (match[3] === undefined || source?.split('#')[0] === match[3]),
      )
    : [];
  if (candidates.length > 1 && match?.[3] === undefined) {
    const local = candidates.filter(({ source }) => source === null);
    if (local.length === 1) candidates = local;
  }
  if (candidates.length === 0) throw new Error(`${path}: ${entry} names no package`);
  if (candidates.length > 1) throw new Error(`${path}: ${entry} names more than one package`);
  return candidates[0];
}
