import type { Asset } from './types';

const loaded = new Map<string, Promise<void>>();

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
        if (view.getUint16(p + 6) !== nameId || view.getUint16(p) !== 3) continue;
        const start = strings + view.getUint16(p + 10);
        return new TextDecoder('utf-16be').decode(bytes.slice(start, start + view.getUint16(p + 8)));
      }
    }
  }
  throw new Error('Font has no supported OpenType family name. Use a font CSS URL.');
}

export function loadFonts(assets: Asset[], resolve: (asset: Asset) => string): Promise<void[]> {
  const fontSet = 'document' in globalThis ? document.fonts : (globalThis as unknown as { fonts: FontFaceSet }).fonts;
  return Promise.all(assets.map(asset => {
    const url = resolve(asset);
    let pending = loaded.get(url);
    if (!pending) {
      pending = (async () => {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`Could not load font: ${url} (${response.status}).`);
        if (response.headers.get('content-type')?.includes('text/css') || /fonts\.googleapis\.com|\.css(?:\?|$)/.test(url)) {
          const css = await response.text();
          await Promise.all(Array.from(css.matchAll(/@font-face\s*\{([^}]+)\}/g), async ([, block]) => {
            const property = (name: string) => block.match(new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;]+)`))?.[1].trim();
            const family = property('font-family')?.replace(/^['"]|['"]$/g, '');
            const src = property('src')?.replace(/url\(([^)]+)\)/g, (_, location: string) => `url(${JSON.stringify(new URL(location.replace(/^['"]|['"]$/g, ''), url).href)})`);
            if (!family || !src) throw new Error(`Invalid font CSS: ${url}`);
            const font = new FontFace(family, src, {
              weight: property('font-weight') ?? 'normal', style: property('font-style') ?? 'normal',
              unicodeRange: property('unicode-range') ?? 'U+0-10FFFF',
            });
            fontSet.add(await font.load());
          }));
        } else {
          const bytes = await response.arrayBuffer();
          fontSet.add(await new FontFace(fontName(bytes), bytes).load());
        }
      })();
      loaded.set(url, pending);
      void pending.catch(() => loaded.delete(url));
    }
    return pending;
  }));
}
