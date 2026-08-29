// Regenerates examples/assets/lipsync-fixture.psd (and .pfv), the small
// committed stand-in for a real multi-expression "tachie" PSD used by the
// character lip-sync demo and the react-bridge integration tests.
//
// Like real toolbox PSDs, every folder is saved hidden — the base portrait
// only composes when a preset (the .pfv) makes layers visible. The face
// mouth shapes sit at their true small sizes/positions so the renderer's
// real-coordinate compositing is exercised.
//
//   node packages/react/scripts/make-lipsync-fixture.mjs
//
// `ag-psd` is a devDependency needed only for this script.

import { writePsdBuffer } from 'ag-psd';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ASSETS = join(HERE, '..', '..', '..', 'examples', 'assets');

const WIDTH = 240;
const HEIGHT = 320;

/** A solid RGBA rectangle as an ag-psd `imageData` object. */
function rect(w, h, [r, g, b, a = 255]) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i += 1) {
    data[i * 4] = r;
    data[i * 4 + 1] = g;
    data[i * 4 + 2] = b;
    data[i * 4 + 3] = a;
  }
  return { width: w, height: h, data };
}

function layer(name, left, top, w, h, color, extra = {}) {
  return { name, left, top, right: left + w, bottom: top + h, imageData: rect(w, h, color), ...extra };
}

const psd = {
  width: WIDTH,
  height: HEIGHT,
  children: [
    {
      name: 'back-hair',
      opened: false,
      hidden: true,
      children: [layer('long', 40, 30, 160, 240, [120, 70, 40, 255])],
    },
    {
      name: 'body',
      opened: false,
      hidden: true,
      children: [
        layer('base', 70, 60, 100, 220, [250, 224, 205, 255]),
        layer('outfit-navy', 56, 176, 128, 120, [40, 60, 130, 255], { hidden: true }),
        layer('outfit-red', 56, 176, 128, 120, [170, 50, 60, 255], { hidden: true }),
      ],
    },
    {
      name: 'face',
      opened: false,
      hidden: true,
      children: [
        {
          name: 'eyes',
          opened: false,
          hidden: true,
          children: [
            layer('open', 90, 104, 60, 16, [40, 40, 60, 255]),
            layer('closed', 90, 112, 60, 3, [40, 40, 60, 255], { hidden: true }),
          ],
        },
        {
          name: 'brows',
          opened: false,
          hidden: true,
          children: [
            layer('normal', 88, 94, 64, 5, [90, 60, 40, 255]),
            layer('angry', 88, 96, 64, 8, [90, 60, 40, 255], { hidden: true }),
          ],
        },
        {
          name: 'mouth',
          opened: false,
          hidden: true,
          children: [
            // Distinct small sizes/offsets so real-coordinate compositing shows.
            layer('closed', 112, 150, 16, 3, [180, 90, 90, 255]),
            layer('a', 108, 142, 24, 20, [170, 70, 70, 255]),
            layer('i', 106, 148, 28, 7, [170, 70, 70, 255]),
            layer('u', 114, 144, 12, 13, [170, 70, 70, 255]),
            layer('e', 108, 146, 24, 11, [170, 70, 70, 255]),
            layer('o', 112, 142, 16, 18, [170, 70, 70, 255]),
          ],
        },
      ],
    },
    {
      name: 'front-hair',
      opened: false,
      hidden: true,
      children: [layer('bangs', 60, 40, 120, 60, [140, 85, 50, 255])],
    },
  ],
};

mkdirSync(ASSETS, { recursive: true });
writeFileSync(join(ASSETS, 'lipsync-fixture.psd'), Buffer.from(writePsdBuffer(psd, { generateThumbnail: false })));

// A PSDTool "all layer" preset for the neutral standing pose: base body +
// navy outfit + both hair layers + open eyes + normal brows. The mouth is
// left out — the demo drives it from audio.
const pose = [
  'back-hair',
  'back-hair/long',
  'body',
  'body/base',
  'body/outfit-navy',
  'face',
  'face/eyes',
  'face/eyes/open',
  'face/brows',
  'face/brows/normal',
  'front-hair',
  'front-hair/bangs',
];
const pfv = [
  '[PSDToolFavorites-v1]',
  'root-name/lipsync-fixture',
  'faview-mode/0',
  '',
  '//neutral',
  ...pose.map((p) => `/${p}`),
  '',
].join('\n');
writeFileSync(join(ASSETS, 'lipsync-fixture.pfv'), pfv);

console.log('wrote examples/assets/lipsync-fixture.psd and .pfv');
