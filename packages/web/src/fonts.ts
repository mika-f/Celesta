import type { Asset } from './types';

type FontLoad = { promise: Promise<void>; ready: boolean; faces: FontFace[] };
const loaded = new Map<string, FontLoad>();
const unresolved = new Set<string>();
const fontSet = () => 'document' in globalThis ? document.fonts : (globalThis as unknown as { fonts: FontFaceSet }).fonts;

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

export function releaseFonts(urls: Iterable<string>): void {
  for (const url of urls) {
    for (const face of loaded.get(url)?.faces ?? []) fontSet().delete(face);
    loaded.delete(url);
  }
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
      const entry: FontLoad = { promise: Promise.resolve(), ready: false, faces: [] };
      loaded.set(url, entry);
      const add = async (face: FontFace) => {
        await face.load();
        if (loaded.get(url) === entry) { fontSet().add(face); entry.faces.push(face); }
      };
      entry.promise = (async () => {
        try {
          const response = await fetch(url);
          if (!response.ok) throw new Error(`Could not load font: ${url} (${response.status}).`);
          if (response.headers.get('content-type')?.includes('text/css') || /fonts\.googleapis\.com|\.css(?:\?|$)/.test(url)) {
            const css = await response.text();
            const blocks = Array.from(css.matchAll(/@font-face\s*\{([^}]+)\}/g));
            if (!blocks.length) throw new Error(`No font faces in CSS: ${url}`);
            await Promise.all(blocks.map(async ([, block]) => {
              try {
                const property = (name: string) => block.match(new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;]+)`))?.[1].trim();
                const family = property('font-family')?.replace(/^['"]|['"]$/g, '');
                const src = property('src')?.replace(/url\(([^)]+)\)/g, (_, location: string) => `url(${JSON.stringify(new URL(location.replace(/^['"]|['"]$/g, ''), url).href)})`);
                if (!family || !src) throw new Error(`Invalid font CSS: ${url}`);
                await add(new FontFace(family, src, {
                  weight: property('font-weight') ?? 'normal', style: property('font-style') ?? 'normal',
                  unicodeRange: property('unicode-range') ?? 'U+0-10FFFF',
                }));
              } catch (cause) { console.warn(`Could not load a font face from ${url}; using the default font.`, cause); }
            }));
          } else {
            const bytes = await response.arrayBuffer();
            await add(new FontFace(fontName(bytes), bytes));
          }
        } catch (cause) { console.warn(`Could not load font ${url}; using the default font.`, cause); }
        finally { entry.ready = true; }
      })();
      pending = entry;
    }
    return pending.promise;
  }));
}
