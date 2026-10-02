// Regenerate the tiny PSD used by the renderer's blend-mode regression tests:
//   node packages/react/scripts/make-psd-blend-fixture.mjs
import { writePsdBuffer } from 'ag-psd';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const modes = ['normal', 'multiply', 'screen', 'overlay', 'linear dodge', 'difference', 'soft light'];
const width = 4;
const height = modes.length;
const backdrop = [64, 128, 192];
const source = [192, 128, 64];
// Columns cover opaque pixels, partial source/backdrop alpha, a transparent
// backdrop, and a transparent source. Layer opacity is independent of alpha.
const baseRow = [255, 128, 0, 255].flatMap((alpha) => [...backdrop, alpha]);
const topRow = [255, 128, 255, 0].flatMap((alpha) => [...source, alpha]);
const psd = {
  width,
  height,
  children: [
    {
      name: 'base',
      imageData: {
        width,
        height,
        data: new Uint8ClampedArray(Array.from({ length: height }, () => baseRow).flat()),
      },
    },
    ...modes.map((blendMode, top) => ({
      name: blendMode,
      blendMode,
      opacity: 128 / 255,
      left: 0,
      top,
      right: width,
      bottom: top + 1,
      imageData: { width, height: 1, data: new Uint8ClampedArray(topRow) },
    })),
  ],
};

const output = fileURLToPath(new URL('../../../examples/assets/psd-blend-fixture.psd', import.meta.url));
writeFileSync(output, writePsdBuffer(psd, { generateThumbnail: false }));
console.log(`wrote ${output}`);
