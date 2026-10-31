import type { Asset } from './types';

type FontLoad = { promise: Promise<void>; ready: boolean; faces: FontFace[] };
const loaded = new Map<string, FontLoad>();
const unresolved = new Set<string>();
const fontSet = () => 'document' in globalThis ? document.fonts : (globalThis as unknown as { fonts: FontFaceSet }).fonts;
let fontSetVersion = 0;

/**
 * Each loaded face's source, its file or its `local()` names, and
 * descriptors, so `featureFamily` can make a copy of it.
 */
const faceSources = new WeakMap<FontFace, { source: ArrayBuffer | string; descriptors: FontFaceDescriptors }>();
/** `featureFamily`'s faces, and the family they copy, by the family name it gave them. */
const featureFaces = new Map<string, { family: string; faces: FontFace[] }>();

/**
 * `format()` keywords and strings, and `tech()` values, the native renderer
 * reads, so both renderers load the same file from a font stylesheet. Keep in
 * step with `SUPPORTED_FORMATS`, `SUPPORTED_FORMAT_STRINGS`, and
 * `SUPPORTED_TECHS` in crates/remote/src/stylesheet.rs; the shared fixture in
 * test/fixtures/font-face-src/ checks both.
 */
const supportedFormats = new Set(['woff2', 'woff', 'truetype', 'opentype']);
const supportedFormatStrings = new Set([...supportedFormats, 'woff2-variations', 'woff-variations', 'truetype-variations', 'opentype-variations']);
const supportedTechs = new Set(['features-opentype', 'features-aat', 'variations', 'color-colrv0', 'color-sbix', 'color-cbdt']);

/** `css` without its comments. */
function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?(?:\*\/|$)/g, '');
}

function unquote(value: string): string {
  return value.trim().replace(/^['"]|['"]$/g, '').trim();
}

/** Splits `text` on `separator` outside quotes and parentheses. */
function splitOutsideNesting(text: string, separator: string): string[] {
  const parts: string[] = [];
  let depth = 0, quote: string | undefined, start = 0;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (quote) { if (char === quote) quote = undefined; }
    else if (char === '"' || char === "'") quote = char;
    else if (char === '(') depth++;
    else if (char === ')') depth = Math.max(0, depth - 1);
    else if (char === separator && depth === 0) { parts.push(text.slice(start, index)); start = index + 1; }
  }
  parts.push(text.slice(start));
  return parts;
}

/** For `text` starting with `name(...)`, the argument as written and the text after it. */
function functionArgument(text: string, name: string): [string, string] | undefined {
  if (text.slice(0, name.length + 1).toLowerCase() !== `${name}(`) return undefined;
  let quote: string | undefined;
  for (let index = name.length + 1; index < text.length; index++) {
    const char = text[index];
    if (quote) { if (char === quote) quote = undefined; }
    else if (char === '"' || char === "'") quote = char;
    else if (char === ')') return [text.slice(name.length + 1, index), text.slice(index + 1)];
  }
  return undefined;
}

/**
 * The entries of a `src` value Celesta supports, in order: a url, or a
 * `local()` font's name. Entries that fail to parse or carry an unsupported
 * `format()` or `tech()` are dropped, as CSS Fonts 4 parses `src`.
 */
function supportedSources(value: string): (string | { local: string })[] {
  return splitOutsideNesting(value, ',').flatMap((entry): (string | { local: string })[] => {
    entry = entry.trim();
    const local = functionArgument(entry, 'local');
    if (local) {
      // Unquoted, a name is identifiers joined by one space.
      const name = /^\s*['"]/.test(local[0]) ? unquote(local[0]) : local[0].trim().split(/\s+/).join(' ');
      return local[1].trim() || !name ? [] : [{ local: name }];
    }
    const url = functionArgument(entry, 'url');
    const location = url && unquote(url[0]);
    if (!url || !location) return [];
    let rest = url[1];
    const format = functionArgument(rest.trimStart(), 'format');
    if (format) {
      // The `*-variations` forms exist only as strings.
      const supported = /^\s*['"]/.test(format[0]) ? supportedFormatStrings : supportedFormats;
      if (!supported.has(unquote(format[0]).toLowerCase())) return [];
      rest = format[1];
    }
    const tech = functionArgument(rest.trimStart(), 'tech');
    if (tech) {
      if (!tech[0].split(',').every(value => supportedTechs.has(value.trim().toLowerCase()))) return [];
      rest = tech[1];
    }
    return rest.trim() ? [] : [location];
  });
}

/** Drops every `featureFamily` face, so the next ones copy the fonts loaded now. */
function dropFeatureFaces(): void {
  if (featureFaces.size === 0) return;
  for (const { faces } of featureFaces.values()) for (const face of faces) fontSet().delete(face);
  featureFaces.clear();
  fontSetVersion++;
}

/**
 * A family drawing `family`'s loaded faces with `settings`, a CSS
 * `font-feature-settings` value, under `label`. Canvas text has no
 * font-feature-settings of its own, so each family and feature set gets
 * copies of its faces with the `featureSettings` descriptor. A copy has
 * the face's source: its file, or the installed fonts its stylesheet's
 * `local()` names, as Chromium applies the descriptor to both. A face made
 * from a file is ready at once in Chromium; elsewhere, and for `local()`
 * names, `featureFacesLoaded()` waits for it. Throws for a family with no
 * face loaded with `<Font>`, as an installed family has no faces to copy.
 */
export function featureFamily(family: string, settings: string, label: string): string {
  const name = `${family} [celesta-features ${label}]`;
  if (featureFaces.has(name)) return name;
  const sources = [...loaded.values()].flatMap(entry => entry.faces).filter(face => face.family === family);
  if (sources.length === 0) {
    throw new Error(`fontFeatures need "${family}" loaded with <Font>: the browser renderer cannot apply OpenType features to installed fonts by their family name.`);
  }
  const faces = sources.map(face => {
    const { source, descriptors } = faceSources.get(face)!;
    return withSource(name, source, { ...descriptors, featureSettings: settings });
  });
  for (const face of faces) {
    fontSet().add(face);
    // A face from `local()` names stays unloaded until asked; its failure
    // is reported by `featureFacesLoaded()`.
    if (face.status === 'unloaded') face.load().catch(() => {});
  }
  fontSetVersion++;
  featureFaces.set(name, { family, faces });
  return name;
}

/** Waits for the faces `featureFamily` made, where they do not load at once. */
export async function featureFacesLoaded(): Promise<void> {
  await Promise.all([...featureFaces.values()].flatMap(({ family, faces }) => faces.map(face => face.loaded.catch(cause => {
    // The face itself loaded, so this is rare; Safari, for one, matches
    // only fonts that come with the operating system.
    const source = faceSources.get(face)?.source;
    throw new Error(`fontFeatures cannot use "${family}": this browser did not load ${typeof source === 'string' ? source : 'its font file'} for it.`, { cause });
  }))));
}

/** Whether a face `featureFamily` made is still loading. */
export function featureFacesPending(): boolean {
  return [...featureFaces.values()].some(({ faces }) => faces.some(face => face.status !== 'loaded'));
}

/**
 * A family that never matches, named after the worker's font set as it is
 * now; `cssFont` puts it after `sans-serif`. Chromium's workers keep each
 * canvas font string's resolution after their FontFaceSet changes, so a
 * string resolved before a face was added keeps measuring with the
 * fallback, and one resolved while a face was there keeps it after its
 * removal. A new name per change makes a new string. Documents drop the
 * resolution, so their strings stay as written.
 */
export function fontSetFamily(): string | undefined {
  return fontSetVersion === 0 || 'document' in globalThis ? undefined : `celesta-font-set-${fontSetVersion}`;
}

/** A face from `source`, kept in `faceSources` for `featureFamily` to copy. */
function withSource(family: string, source: ArrayBuffer | string, descriptors: FontFaceDescriptors = {}): FontFace {
  const face = new FontFace(family, source, descriptors);
  faceSources.set(face, { source, descriptors });
  return face;
}

function fontName(bytes: ArrayBuffer): string {
  const view = new DataView(bytes);
  for (let table = 0; table < view.getUint16(4); table++) {
    const at = 12 + table * 16;
    if (view.getUint32(at) !== 0x6e616d65) continue; // OpenType "name" table.
    const offset = view.getUint32(at + 8);
    const strings = offset + view.getUint16(offset + 4);
    for (const nameId of [16, 1]) {
      for (let record = 0; record < view.getUint16(offset + 2); record++) {
        const p = offset + 6 + record * 12;
        if (view.getUint16(p + 6) !== nameId || ![0, 3].includes(view.getUint16(p))) continue;
        const start = strings + view.getUint16(p + 10);
        return new TextDecoder('utf-16be').decode(bytes.slice(start, start + view.getUint16(p + 8)));
      }
    }
  }
  throw new Error('Font has no supported OpenType family name. Use a font CSS URL.');
}

/** Synchronous hooks may only use fonts already loaded by prepare() or <Font>. */
export function requireLoadedFonts(assets: Asset[], resolve: (asset: Asset) => string): void {
  const missing = assets.some(asset => {
    try { return !loaded.get(resolve(asset))?.ready; }
    catch { return !unresolved.has(JSON.stringify(asset.location)); }
  });
  if (missing) throw new Error('Browser text measurements require preloaded fonts. Call await measureText(..., { fonts }) in prepare() before useTextMetrics(..., { fonts }).');
}

/**
 * The families the fonts in `assets` added, in list order without
 * duplicates: what `fontFallback: 'loaded'` text falls back to.
 */
export function loadedFamilies(assets: readonly Asset[], resolve: (asset: Asset) => string): string[] {
  const families: string[] = [];
  for (const asset of assets) {
    let url: string;
    try { url = resolve(asset); }
    catch { continue; }
    for (const face of loaded.get(url)?.faces ?? []) {
      if (!families.includes(face.family)) families.push(face.family);
    }
  }
  return families;
}

export function releaseFonts(urls: Iterable<string>): void {
  for (const url of urls) {
    for (const face of loaded.get(url)?.faces ?? []) { fontSet().delete(face); fontSetVersion++; }
    loaded.delete(url);
  }
  dropFeatureFaces();
}

/** The dedicated worker owns a single project's font set. */
export function resetFonts(): void {
  releaseFonts(loaded.keys());
  unresolved.clear();
}

export function loadFonts(assets: Asset[], resolve: (asset: Asset) => string): Promise<void[]> {
  return Promise.all(assets.map(asset => {
    let url: string;
    try { url = resolve(asset); }
    catch (cause) {
      const key = JSON.stringify(asset.location);
      if (!unresolved.has(key)) console.warn('Could not resolve font; using the default font.', cause);
      unresolved.add(key);
      return Promise.resolve();
    }
    let pending = loaded.get(url);
    if (!pending) {
      const google = /^https:\/\/fonts\.googleapis\.com\/css2?\?/.test(url);
      const entry: FontLoad = { promise: Promise.resolve(), ready: false, faces: [] };
      loaded.set(url, entry);
      const add = async (face: FontFace) => {
        await face.load();
        if (loaded.get(url) === entry) { fontSet().add(face); fontSetVersion++; entry.faces.push(face); dropFeatureFaces(); }
      };
      entry.promise = (async () => {
        try {
          const response = await fetch(url);
          if (google && response.status === 400) {
            const family = new URL(url).searchParams.get('family')?.split(':')[0];
            throw new Error(`Google Fonts could not find family "${family}" or the requested weights. Check the name and available weights at https://fonts.google.com.`);
          }
          if (!response.ok) throw new Error(`Could not load font: ${url} (${response.status}).`);
          if (response.headers.get('content-type')?.includes('text/css') || /fonts\.googleapis\.com|\.css(?:\?|$)/.test(url)) {
            // As the native renderer reads it: comments may hold `}`, `;`,
            // or quotes that would cut the blocks and declarations short.
            const css = stripComments(await response.text());
            const blocks = Array.from(css.matchAll(/@font-face\s*\{([^}]+)\}/gi));
            if (!blocks.length) throw new Error(`No font faces in CSS: ${url}`);
            await Promise.all(blocks.map(async ([, block]) => {
              try {
                // Descriptor names ignore case, and a later one wins.
                const declarations = splitOutsideNesting(block, ';').flatMap(declaration => {
                  const [, name, value] = /^\s*([\w-]+)\s*:([\s\S]*)$/.exec(declaration) ?? [];
                  return name ? [[name.toLowerCase(), value.trim()] as const] : [];
                }).reverse();
                const property = (name: string) => declarations.find(([key]) => key === name)?.[1];
                const family = property('font-family')?.replace(/^['"]|['"]$/g, '');
                if (!family) throw new Error(`Invalid font CSS: ${url}`);
                // The last valid `src` wins; one with no supported entry is
                // invalid, so an earlier one stays in effect. Its first url()
                // is the file to load, even after a local(). With none, the
                // installed fonts its local() faces name go to FontFace in
                // the order the native renderer tries them, and the browser
                // matches them by its own rules.
                const src = declarations
                  .filter(([key]) => key === 'src')
                  .map(([, value]) => supportedSources(value))
                  .find(sources => sources.length);
                if (!src) throw new Error(`@font-face "${family}" in ${url} has no src entry Celesta can use (a local() font name, or a TrueType, OpenType, WOFF, or WOFF2 file).`);
                const location = src.find((source): source is string => typeof source === 'string');
                const descriptors: FontFaceDescriptors = {
                  weight: property('font-weight') ?? 'normal', style: property('font-style') ?? 'normal',
                  unicodeRange: property('unicode-range') ?? 'U+0-10FFFF',
                };
                if (location == null) {
                  const locals = src
                    .filter((source): source is { local: string } => typeof source !== 'string')
                    .map(({ local }) => `local("${local.replace(/["\\]/g, '\\$&')}")`);
                  await add(withSource(family, locals.join(', '), descriptors));
                }
                else {
                  const file = new URL(location, url).href;
                  const response = await fetch(file);
                  if (!response.ok) throw new Error(`Could not load font: ${file} (${response.status}).`);
                  await add(withSource(family, await response.arrayBuffer(), descriptors));
                }
              } catch (cause) {
                if (google) throw cause;
                console.warn(`Could not load a font face from ${url}; using the default font.`, cause);
              }
            }));
          } else {
            const bytes = await response.arrayBuffer();
            await add(withSource(fontName(bytes), bytes));
          }
        } catch (cause) {
          if (google) {
            if (loaded.get(url) === entry) releaseFonts([url]);
            throw cause;
          }
          console.warn(`Could not load font ${url}; using the default font.`, cause);
        }
        finally { entry.ready = true; }
      })();
      pending = entry;
    }
    return pending.promise;
  }));
}
