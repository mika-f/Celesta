// Celesta's version, the root Cargo.toml's [workspace.package] version, and
// the plugins that give it too: each such plugin's Cargo package and its
// plugin.json carry Celesta's version, and bump-version.mts bumps them all
// together. The rewrites are line by line, so every other byte stays.

/** The plugin folders, relative to the repository root, that give Celesta's version. */
export const FOLLOWS_CELESTA = ['plugins/avfoundation', 'plugins/ffmpeg'];

const HEADER = /^\s*\[\[?\s*([^\]]*?)\s*\]\]?\s*(?:#.*)?$/;
const VERSION = /^(\s*version\s*=\s*")([^"]*)(".*)$/s;

/** `text` as a regular expression matching it literally. */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Calls `onLine` with each line of `cargoToml` and the table it is in (`''`
 * before the first header), and joins what it returns.
 */
function eachLine(cargoToml: string, onLine: (line: string, table: string) => string): string {
  let table = '';
  return cargoToml
    .split('\n')
    .map((line) => {
      const header = HEADER.exec(line);
      if (header) {
        table = header[1];
        return line;
      }
      return onLine(line, table);
    })
    .join('\n');
}

/** The `version` of the root Cargo.toml's `[workspace.package]`. */
export function workspaceVersion(cargoToml: string): string {
  let version: string | undefined;
  eachLine(cargoToml, (line, table) => {
    const match = table === 'workspace.package' ? VERSION.exec(line) : null;
    if (match && version === undefined) version = match[2];
    return line;
  });
  if (version === undefined) throw new Error('no [workspace.package] version');
  return version;
}

/**
 * `cargoToml` (read from `path`, which errors name) with its
 * `[workspace.package]` or `[package]` version and every pin on one of
 * Celesta's crates (`{ path = "...", version = "=<from>" }`) moved from `from`
 * to `to`. Comments and crates without a `path` are left as they are.
 * A pin must be a one-line inline table: the scan reads one line at a time.
 */
export function bumpCargoToml(cargoToml: string, from: string, to: string, path: string): string {
  const pin = new RegExp(`(\\bversion\\s*=\\s*")=${escapeRegExp(from)}(")`, 'g');
  let versionTable: string | undefined;
  let versions = 0;
  const text = eachLine(cargoToml, (line, table) => {
    if (table !== 'workspace.package' && table !== 'package') {
      if (/^\s*#/.test(line) || !/\bpath\s*=/.test(line)) return line;
      return line.replace(pin, (_, before, after) => `${before}=${to}${after}`);
    }
    versionTable ??= table;
    const match = VERSION.exec(line);
    if (!match) return line;
    if (match[2] !== from) throw new Error(`${path}: version ${match[2]} is not Celesta's ${from}`);
    versions += 1;
    return `${match[1]}${to}${match[3]}`;
  });
  if (versions === 0) throw new Error(`${path}: no [${versionTable ?? 'package'}] version`);
  return text;
}

/**
 * `pluginJson` (read from `path`, which errors name) with its `version` moved
 * from `from` to `to`.
 */
export function bumpPluginJson(pluginJson: string, from: string, to: string, path: string): string {
  const { version }: { version?: unknown } = JSON.parse(pluginJson);
  if (version !== from) throw new Error(`${path}: version ${version} is not Celesta's ${from}`);
  const field = /("version"\s*:\s*")([^"]*)(")/g;
  const matches = [...pluginJson.matchAll(field)];
  if (matches.length !== 1) throw new Error(`${path}: expected one "version" field, found ${matches.length}`);
  return pluginJson.replace(field, (_, before, _version, after) => `${before}${to}${after}`);
}
