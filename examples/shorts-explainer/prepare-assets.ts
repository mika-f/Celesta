// Downloads the official 東北ずん子 and 東北きりたん PSDs from zunko.jp
// (東北ずん子・ずんだもんプロジェクト) into assets/. The material is not part
// of this repository: its terms allow free non-commercial use but not
// redistribution (https://zunko.jp/guideline.html).
//
// The originals are large (きりたん is 5501×8500), and Celesta rasterizes a
// PSD portrait once per mouth and eye combination, so every layer is shrunk
// to a 2400 px tall canvas here. Layer names, order, visibility, opacity and
// blend modes are kept, so the layer paths in character.ts still match.
//
//   pnpm install            # once, for ag-psd
//   node examples/shorts-explainer/prepare-assets.ts
//
// Needs Node.js 23.6 or later. Existing files in assets/ are kept; delete
// them to fetch again.

import { access, mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { initializeCanvas, readPsd, writePsdBuffer } from 'ag-psd';
import type { Layer, Psd } from 'ag-psd';

// Layers are read and written as raw pixels; no canvas is ever drawn.
initializeCanvas(
  () => { throw new Error('prepare-assets.ts does not draw on a canvas'); },
  (width, height) => ({ width, height, data: new Uint8ClampedArray(width * height * 4) }) as ImageData,
);

const SOURCES = [
  { file: 'zunko.psd', url: 'https://zunko.jp/sozai/zunkot/a1zunko11.psd' },
  { file: 'kiritan.psd', url: 'https://zunko.jp/sozai/zunkot/a1zunko75.psd' },
];
const HEIGHT = 2400;
const OUT = join(import.meta.dirname, 'assets');

type Pixels = { width: number; height: number; data: Uint8ClampedArray };
type Bounds = { left?: number; top?: number; right?: number; bottom?: number };

// Area-average downscale of `image` placed at (left, top) in the document,
// onto the scaled document's pixel grid. Colors are averaged premultiplied,
// so transparent edges do not darken. Returns the new image and its bounds.
function shrink(image: Pixels, left: number, top: number, scale: number) {
  const x0 = Math.floor(left * scale);
  const y0 = Math.floor(top * scale);
  const x1 = Math.max(x0 + 1, Math.ceil((left + image.width) * scale));
  const y1 = Math.max(y0 + 1, Math.ceil((top + image.height) * scale));
  const width = x1 - x0;
  const height = y1 - y0;
  const out = new Uint8ClampedArray(width * height * 4);
  const src = image.data;
  for (let y = 0; y < height; y++) {
    const sy0 = Math.max(0, Math.floor((y0 + y) / scale - top));
    const sy1 = Math.min(image.height, Math.max(sy0 + 1, Math.floor((y0 + y + 1) / scale - top)));
    for (let x = 0; x < width; x++) {
      const sx0 = Math.max(0, Math.floor((x0 + x) / scale - left));
      const sx1 = Math.min(image.width, Math.max(sx0 + 1, Math.floor((x0 + x + 1) / scale - left)));
      let r = 0, g = 0, b = 0, a = 0, n = 0;
      for (let sy = sy0; sy < sy1; sy++) {
        let i = (sy * image.width + sx0) * 4;
        for (let sx = sx0; sx < sx1; sx++, i += 4) {
          const alpha = src[i + 3];
          r += src[i] * alpha;
          g += src[i + 1] * alpha;
          b += src[i + 2] * alpha;
          a += alpha;
          n++;
        }
      }
      const o = (y * width + x) * 4;
      if (a > 0) {
        out[o] = r / a;
        out[o + 1] = g / a;
        out[o + 2] = b / a;
        out[o + 3] = a / Math.max(1, n);
      }
    }
  }
  return { image: { width, height, data: out }, left: x0, top: y0, right: x1, bottom: y1 };
}

function shrinkInto(target: Bounds & { imageData?: Pixels }, scale: number) {
  if (target.imageData && target.imageData.width > 0 && target.imageData.height > 0) {
    const s = shrink(target.imageData, target.left ?? 0, target.top ?? 0, scale);
    Object.assign(target, { imageData: s.image, left: s.left, top: s.top, right: s.right, bottom: s.bottom });
  } else {
    for (const key of ['left', 'top', 'right', 'bottom'] as const) {
      if (target[key] !== undefined) target[key] = Math.round(target[key]! * scale);
    }
  }
}

function shrinkLayers(layers: Layer[], scale: number) {
  for (const layer of layers) {
    shrinkInto(layer as Layer & { imageData?: Pixels }, scale);
    if (layer.mask) shrinkInto(layer.mask as Bounds & { imageData?: Pixels }, scale);
    if (layer.children) shrinkLayers(layer.children, scale);
  }
}

async function exists(path: string) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function main() {
  await mkdir(OUT, { recursive: true });
  for (const { file, url } of SOURCES) {
    const path = join(OUT, file);
    if (await exists(path)) {
      console.log(`${file}: already in assets/`);
      continue;
    }
    console.log(`${file}: downloading ${url}`);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
    const original = new Uint8Array(await response.arrayBuffer());
    const psd: Psd = readPsd(original, {
      useImageData: true, skipThumbnail: true, skipCompositeImageData: true, skipLinkedFilesData: true,
    });
    const scale = HEIGHT / psd.height;
    console.log(`${file}: ${psd.width}×${psd.height} → ${Math.round(psd.width * scale)}×${HEIGHT}`);
    shrinkLayers(psd.children ?? [], scale);
    psd.width = Math.round(psd.width * scale);
    psd.height = HEIGHT;
    await writeFile(path, writePsdBuffer(psd, { generateThumbnail: false }));
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
