// Resolves a PSDTool layer-visibility preset into the flat list of layer
// paths a `<Character>` PSD portrait should compose. The Rust rasterizer
// (`mikan_renderer::rasterize_psd`) renders exactly the leaf layers whose
// full path appears in that list, so this module's job is to turn the
// formats a PSDTool user already has — a copied layer-state string, or a
// `.pfv` favorites file — into `string[]`.
//
// Real "tachie" PSDs (multi-outfit / multi-expression toolbox files) save
// every folder as hidden; without a preset only force-enabled layers (the
// lip-sync mouth) would compose. See HANDOFF.md / the plan for context.

import * as fs from 'node:fs';
import * as path from 'node:path';

import { entryRelativePath } from './entry-dir';

/** One named favorite from a `.pfv` file: its layer-state string plus the
 *  folder path it lives under in PSDTool's favorites tree. */
export interface PfvFavorite {
  /** Leaf name shown in PSDTool (last path segment). */
  name: string;
  /** Full favorites-tree path, e.g. `poses/casual`. */
  path: string;
  /** The raw PSDTool layer-state string for this favorite (may be empty for folders). */
  state: string;
}

export interface ParsedPfv {
  rootName: string;
  favorites: PfvFavorite[];
}

/** Reverses PSDTool's `encodeName` (`%XX` for a small set of ASCII chars). */
function decodeSegment(segment: string): string {
  // A raw segment is `encodeName(name)` optionally followed by `\<index>`
  // (PSDTool's disambiguation suffix for sibling layers that share a name).
  // `encodeName` escapes `\`, so an unescaped trailing `\<digits>` can only
  // be that suffix. The Rust side builds paths from bare layer names with no
  // such suffix, so drop it — duplicate sibling names are not disambiguated.
  const withoutIndex = segment.replace(/\\\d+$/, '');
  return withoutIndex.replace(/%([0-9a-fA-F]{2})/g, (_, hex: string) =>
    String.fromCharCode(parseInt(hex, 16)),
  );
}

function decodePath(rawPath: string): string {
  return rawPath
    .split('/')
    .filter((segment) => segment.length > 0)
    .map(decodeSegment)
    .join('/');
}

/**
 * Parses a PSDTool layer-state string into the list of visible layer/folder
 * paths. Accepts both the "all layer" form (every line prefixed with `/`,
 * one line per visible layer — this is what PSDTool's favorites store for a
 * full pose) and the compact form. Unknown-to-the-PSD entries are harmless;
 * the rasterizer ignores paths it cannot match.
 */
export function resolveVisibleLayers(state: string): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const line of state.replace(/\r/g, '').split('\n')) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      continue;
    }
    const decoded = decodePath(trimmed);
    if (decoded.length > 0 && !seen.has(decoded)) {
      seen.add(decoded);
      result.push(decoded);
    }
  }
  return result;
}

/**
 * Parses a `.pfv` (PSDTool favorites) file. The format is a
 * `[PSDToolFavorites-v1]` header followed by blocks: a `//tree/path` line,
 * the (possibly multi-line) layer-state string, then a blank line. Folder
 * entries end their path with `~folder` and carry no state; `~filter`
 * entries carry a filter expression we ignore.
 */
export function parsePfv(text: string): ParsedPfv {
  const lines = text.replace(/^﻿/, '').replace(/\r/g, '').split('\n');
  let rootName = 'Favorites';
  const favorites: PfvFavorite[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.startsWith('root-name/')) {
      rootName = decodeSegment(line.slice('root-name/'.length));
      continue;
    }
    if (!line.startsWith('//')) {
      continue;
    }
    let treePath = line.slice(2);
    let isFilter = false;
    if (treePath.endsWith('~folder')) {
      continue;
    }
    if (treePath.endsWith('~filter')) {
      treePath = treePath.slice(0, -'~filter'.length);
      isFilter = true;
    }
    // The state runs until the next blank line or the next block header.
    const stateLines: string[] = [];
    let j = i + 1;
    for (; j < lines.length && lines[j].length > 0 && !lines[j].startsWith('//'); j += 1) {
      stateLines.push(lines[j]);
    }
    i = j - 1;
    if (isFilter) {
      continue;
    }
    const decodedPath = decodePath(treePath);
    const name = decodedPath.split('/').pop() ?? decodedPath;
    favorites.push({ name, path: decodedPath, state: stateLines.join('\n') });
  }
  return { rootName, favorites };
}

export interface LoadPsdPresetOptions {
  /** `.pfv` path, resolved relative to the entry file's directory (like `<Audio src>`). */
  src: string;
  /** Which favorite to use — its leaf name or full tree path. Defaults to the first with a state. */
  favorite?: string;
}

/**
 * Reads a `.pfv` favorites file and resolves one favorite into the visible
 * layer list a PSD portrait's `layers` prop expects. Call from an entry's
 * `prepare()` export and stash the result in module state.
 */
export async function loadPsdPreset(options: LoadPsdPresetOptions): Promise<string[]> {
  const filePath = entryRelativePath(options.src);
  const text = await fs.promises.readFile(filePath, 'utf8');
  const { favorites } = parsePfv(text);
  const withState = favorites.filter((favorite) => favorite.state.trim().length > 0);
  const chosen = options.favorite
    ? withState.find(
        (favorite) => favorite.path === options.favorite || favorite.name === options.favorite,
      )
    : withState[0];
  if (!chosen) {
    const available = favorites.map((favorite) => favorite.path).join(', ') || '(none)';
    throw new Error(
      options.favorite
        ? `favorite "${options.favorite}" not found in ${path.basename(filePath)} (have: ${available})`
        : `${path.basename(filePath)} has no favorite with a layer state`,
    );
  }
  return resolveVisibleLayers(chosen.state);
}
