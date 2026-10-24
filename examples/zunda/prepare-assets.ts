// このサンプルが使う第三者の素材を assets/ に取得する。素材はリポジトリに
// 含めないので、最初に一度実行する（ffmpeg コマンドが必要）。
//
//   node examples/zunda/prepare-assets.ts
//
// 公式 PSD はとても大きい（ずんだもんは 4832 × 9488）。原寸のままだと、立ち絵
// 1 枚が GPU のテクスチャの上限（環境によって 8192 px）を超えるうえ、読み込みと
// 合成にも時間がかかる。そこで、ここで一度だけレイヤーごとに縮小して PSD に書き戻す。
// 置き場所のファイル名は、映像側と同じ src/theme.ts の ASSET から取る。

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { basename, dirname, join } from 'node:path';

import { ASSET } from './src/theme.ts';

// PSD の読み書きには、packages/react が依存している ag-psd を借りる。
type AgPsd = {
  initializeCanvas(createCanvas: () => never, createImageData: (width: number, height: number) => unknown): void;
  readPsd(buffer: Uint8Array, options: object): unknown;
  writePsd(psd: unknown, options: object): ArrayBuffer;
};
const require = createRequire(join(import.meta.dirname, '../../packages/react/package.json'));
const { initializeCanvas, readPsd, writePsd } = require('ag-psd') as AgPsd;

// 画素はバイト列のまま扱う（useImageData）ので、本物の canvas は要らない。
initializeCanvas(
  () => {
    throw new Error('canvas is not available');
  },
  (width, height) => ({ width, height, data: new Uint8ClampedArray(width * height * 4) }),
);

const HERE = import.meta.dirname;

/** 取得する素材。`target` は src/theme.ts の ASSET（エントリからの相対パス）。`scale` があれば PSD を縮小する。 */
type Source = { target: string; url: string; scale?: number };

const SOURCES: Source[] = [
  // 東北ずん子・ずんだもんプロジェクト公式イラスト（https://zunko.jp/con_illust.html）
  { target: ASSET.zunda, url: 'https://zunko.jp/sozai/zundamon/zunmon008.psd', scale: 0.25 },
  { target: ASSET.metan.happy, url: 'https://zunko.jp/sozai/methane/met_s214.psd', scale: 0.2 },
  { target: ASSET.metan.talk, url: 'https://zunko.jp/sozai/methane/met_s215.psd', scale: 0.2 },
  { target: ASSET.metan.worried, url: 'https://zunko.jp/sozai/methane/met_s219.psd', scale: 0.2 },
  { target: ASSET.metan.what, url: 'https://zunko.jp/sozai/methane/met_s220.psd', scale: 0.2 },
  // Mixkit Stock Video Free License（https://mixkit.co/license/#videoFree）
  { target: ASSET.clip, url: 'https://assets.mixkit.co/videos/8621/8621-720.mp4' },
];

type Pixels = { width: number; height: number; data: Uint8ClampedArray | Uint8Array };

/**
 * 面積平均で縮小する。色は不透明度で重み付けして平均するので、透明な部分に
 * 接する縁が黒ずまない。
 */
function shrink(source: Pixels, scale: number): Pixels {
  const width = Math.max(1, Math.round(source.width * scale));
  const height = Math.max(1, Math.round(source.height * scale));
  // 縮小後の 1 画素ごとに、R・G・B（不透明度で重み付け）、不透明度、画素数の合計
  const sums = new Float64Array(width * height * 5);
  for (let y = 0; y < source.height; y++) {
    const ty = Math.min(height - 1, Math.floor(y * scale));
    for (let x = 0; x < source.width; x++) {
      const tx = Math.min(width - 1, Math.floor(x * scale));
      const from = (y * source.width + x) * 4;
      const to = (ty * width + tx) * 5;
      const opacity = source.data[from + 3] / 255;
      sums[to] += source.data[from] * opacity;
      sums[to + 1] += source.data[from + 1] * opacity;
      sums[to + 2] += source.data[from + 2] * opacity;
      sums[to + 3] += opacity;
      sums[to + 4] += 1;
    }
  }
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    const opacity = sums[i * 5 + 3];
    const count = sums[i * 5 + 4] || 1;
    if (opacity > 0) {
      data[i * 4] = sums[i * 5] / opacity;
      data[i * 4 + 1] = sums[i * 5 + 1] / opacity;
      data[i * 4 + 2] = sums[i * 5 + 2] / opacity;
    }
    data[i * 4 + 3] = (opacity / count) * 255;
  }
  return { width, height, data };
}

type Bounds = { top?: number; left?: number; bottom?: number; right?: number };
type PsdLayer = Bounds & { imageData?: Pixels; children?: PsdLayer[]; mask?: Bounds & { imageData?: Pixels } };

/** 画素と位置を `scale` 倍にする。フォルダの中も同じように縮める。 */
function shrinkLayers(layers: PsdLayer[] | undefined, scale: number) {
  for (const layer of layers ?? []) {
    for (const part of [layer, layer.mask]) {
      if (!part?.imageData || part.imageData.width === 0 || part.imageData.height === 0) continue;
      part.imageData = shrink(part.imageData, scale);
      part.left = Math.round((part.left ?? 0) * scale);
      part.top = Math.round((part.top ?? 0) * scale);
      part.right = part.left + part.imageData.width;
      part.bottom = part.top + part.imageData.height;
    }
    shrinkLayers(layer.children, scale);
  }
}

function shrinkPsd(input: Uint8Array, scale: number): Uint8Array {
  const psd = readPsd(input, { useImageData: true, skipThumbnail: true }) as {
    width: number;
    height: number;
    imageData?: Pixels;
    children?: PsdLayer[];
  };
  psd.width = Math.round(psd.width * scale);
  psd.height = Math.round(psd.height * scale);
  if (psd.imageData) psd.imageData = shrink(psd.imageData, scale);
  shrinkLayers(psd.children, scale);
  return new Uint8Array(writePsd(psd, { generateThumbnail: false }));
}

async function download(url: string): Promise<Uint8Array> {
  const response = await fetch(url, { headers: { 'user-agent': 'celesta-example/0.1' } });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

for (const { target, url, scale } of SOURCES) {
  const file = join(HERE, target);
  if (existsSync(file)) {
    console.log(`skip ${target}`);
    continue;
  }
  // 元のファイルは assets/.cache に取っておき、縮小率を変えたときに取り直さずに済むようにする。
  const original = join(HERE, 'assets', '.cache', basename(url));
  mkdirSync(dirname(original), { recursive: true });
  if (!existsSync(original)) {
    console.log(`get  ${url}`);
    writeFileSync(original, await download(url));
  }
  const bytes = readFileSync(original);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, scale ? shrinkPsd(bytes, scale) : bytes);
  console.log(`done ${target}`);
}

// 「写真」として使う静止画は、同じ映像素材の 1 フレームから切り出す。
const still = join(HERE, ASSET.photo);
if (!existsSync(still)) {
  execFileSync('ffmpeg', ['-loglevel', 'error', '-ss', '12', '-i', join(HERE, ASSET.clip), '-frames:v', '1', '-q:v', '2', still]);
  console.log(`done ${ASSET.photo}`);
}
