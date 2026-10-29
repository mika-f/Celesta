import { clamp, clamp01, parseHexColor } from '@celesta/math';
import type { BuiltinFilter, ColorFilter } from './types';

// The built-in filters as the GPU renderer runs them: `color_filter.wgsl`
// and `builtin_effect.wgsl`, each one pass over premultiplied pixels, in
// order, through the passes of its shader chain (`shader.rs`).

/** Left, top, right, bottom, in pixels. */
export type Box = readonly [number, number, number, number];

// Rec. 709 luma, as CSS `saturate()` and `grayscale()` use.
const LR = 0.2126, LG = 0.7152, LB = 0.0722;
// The most output pixels a filter writes beyond what it reads.
const MAX_PADDING = 512;
const ZOOM_SAMPLES = 32;

// Rust's `rem_euclid`, as the renderer reduces angles before uploading them.
const turn = (degrees: number) => ((degrees % 360) + 360) % 360;
const fract = (value: number) => value - Math.floor(value);

function hexColor(color: string): [number, number, number, number] {
  const channels = parseHexColor(color);
  if (!channels) throw new Error(`Invalid color: ${color}`);
  return channels.map(channel => channel / 255) as [number, number, number, number];
}

/** A color filter's change to one straight color, in place: `rgb` 0–1, `u`/`v` the pixel's place in the content box. */
type Step = (rgb: number[], u: number, v: number) => void;

function step(filter: ColorFilter): Step {
  const saturate = (rgb: number[], amount: number) => {
    const luma = rgb[0] * LR + rgb[1] * LG + rgb[2] * LB;
    for (let i = 0; i < 3; i++) rgb[i] = luma + (rgb[i] - luma) * amount;
  };
  switch (filter.type) {
    case 'brightness': {
      const amount = Math.max(filter.amount, 0);
      return rgb => { for (let i = 0; i < 3; i++) rgb[i] *= amount; };
    }
    case 'contrast': {
      const amount = Math.max(filter.amount, 0);
      return rgb => { for (let i = 0; i < 3; i++) rgb[i] = (rgb[i] - 0.5) * amount + 0.5; };
    }
    case 'saturation': {
      const amount = Math.max(filter.amount, 0);
      return rgb => saturate(rgb, amount);
    }
    case 'grayscale': {
      const amount = 1 - clamp01(filter.amount);
      return rgb => saturate(rgb, amount);
    }
    case 'hue': {
      // CSS `hue-rotate()`.
      const angle = filter.degrees * Math.PI / 180, c = Math.cos(angle), s = Math.sin(angle);
      const m = [
        LR + (1 - LR) * c - LR * s, LG - LG * c - LG * s, LB - LB * c + (1 - LB) * s,
        LR - LR * c + 0.143 * s, LG + (1 - LG) * c + 0.140 * s, LB - LB * c - 0.283 * s,
        LR - LR * c - (1 - LR) * s, LG - LG * c + LG * s, LB + (1 - LB) * c + LB * s,
      ];
      return rgb => {
        const [r, g, b] = rgb;
        rgb[0] = m[0] * r + m[1] * g + m[2] * b;
        rgb[1] = m[3] * r + m[4] * g + m[5] * b;
        rgb[2] = m[6] * r + m[7] * g + m[8] * b;
      };
    }
    case 'invert': {
      const amount = clamp01(filter.amount);
      return rgb => { for (let i = 0; i < 3; i++) rgb[i] += (1 - 2 * rgb[i]) * amount; };
    }
    case 'tint': {
      const [r, g, b, a] = hexColor(filter.color);
      const amount = clamp01(filter.amount) * a, color = [r, g, b];
      return rgb => { for (let i = 0; i < 3; i++) rgb[i] += (color[i] - rgb[i]) * amount; };
    }
    case 'vignette': {
      const amount = clamp01(filter.amount), softness = Math.max(filter.softness, 1e-4);
      return (rgb, u, v) => {
        // 0 at the content box's centre, 1 at its corners.
        const distance = Math.hypot(u - 0.5, v - 0.5) * Math.SQRT2;
        const t = clamp01((distance - filter.radius) / softness);
        const scale = 1 - amount * t * t * (3 - 2 * t);
        for (let i = 0; i < 3; i++) rgb[i] *= scale;
      };
    }
    default:
      throw new Error(`Unsupported filter: ${(filter as { type: string }).type}`);
  }
}

/** How far `filter` may write beyond what it reads, as `ShaderProcessor::use_filters` reckons it. */
function padding(filter: BuiltinFilter, diagonal: number): number {
  let reach = 0;
  switch (filter.type) {
    case 'pixelate': reach = filter.size; break;
    case 'halftone': reach = filter.size * Math.SQRT2; break;
    case 'chromaticAberration': reach = Math.abs(filter.offset); break;
    case 'wave': reach = Math.abs(filter.amplitude); break;
    case 'zoomBlur': {
      // A pixel shows what lies `1 - amount` as far from the centre.
      const amount = clamp01(filter.amount);
      reach = amount < 1 ? amount / (1 - amount) * diagonal : MAX_PADDING;
      break;
    }
  }
  return clamp(reach, 0, MAX_PADDING);
}

/** How far `filters` together may write beyond what they read, in a scene whose diagonal is `diagonal`. */
export function filtersPadding(filters: readonly BuiltinFilter[], diagonal: number): number {
  return filters.reduce((sum, filter) => sum + padding(filter, diagonal), 0);
}

/** PCG's output permutation of `v`'s state step: a 32-bit integer hash, as `builtin_effect.wgsl`'s `pcg`. */
function pcg(v: number): number {
  const state = (Math.imul(v, 747796405) + 2891336453) >>> 0;
  const word = Math.imul((state >>> ((state >>> 28) + 4)) ^ state, 277803737) >>> 0;
  return ((word >>> 22) ^ word) >>> 0;
}

/** A uniform value in [0, 1) for a pixel, the seed, and a channel: the GPU's exactly. */
function random(x: number, y: number, seed: number, channel: number): number {
  const hash = pcg((x >>> 0) ^ pcg((y >>> 0) ^ pcg((seed + Math.imul(channel, 0x9e3779b9)) >>> 0)));
  return (hash >>> 8) / 16777216;
}

/** BT.709 Cb and Cr of a straight sRGB-encoded color. */
function chroma(r: number, g: number, b: number): [number, number] {
  const y = r * LR + g * LG + b * LB;
  return [(b - y) / 1.8556, (r - y) / 1.5748];
}

/**
 * Premultiplied pixels of a region of the surface: `left`, `top` is its
 * top-left pixel. Outside it, pixels are transparent, as outside a canvas
 * on the GPU.
 */
class Region {
  readonly data: Float32Array;
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;

  constructor(box: Box) {
    [this.left, this.top] = box;
    this.width = box[2] - box[0];
    this.height = box[3] - box[1];
    this.data = new Float32Array(this.width * this.height * 4);
  }

  /** Adds `weight` times the pixel at `x`, `y` (surface pixels) to `sum`: `source_load`. */
  load(x: number, y: number, weight: number, sum: number[]): void {
    const column = x - this.left, row = y - this.top;
    if (column < 0 || row < 0 || column >= this.width || row >= this.height || weight === 0) return;
    const at = (row * this.width + column) * 4;
    sum[0] += this.data[at] * weight;
    sum[1] += this.data[at + 1] * weight;
    sum[2] += this.data[at + 2] * weight;
    sum[3] += this.data[at + 3] * weight;
  }

  /** Adds `weight` times the pixels at `x`, `y` interpolated bilinearly between pixel centres: `source_at`. */
  sample(x: number, y: number, weight: number, sum: number[]): void {
    const atX = x - 0.5, atY = y - 0.5;
    const left = Math.floor(atX), top = Math.floor(atY);
    const fx = atX - left, fy = atY - top;
    this.load(left, top, (1 - fx) * (1 - fy) * weight, sum);
    this.load(left + 1, top, fx * (1 - fy) * weight, sum);
    this.load(left, top + 1, (1 - fx) * fy * weight, sum);
    this.load(left + 1, top + 1, fx * fy * weight, sum);
  }
}

/** Turns premultiplied `color` straight, in place; transparent becomes 0. */
function unpremultiply(color: number[]): void {
  const alpha = color[3];
  if (alpha <= 0) {
    color.fill(0);
    return;
  }
  for (let i = 0; i < 3; i++) color[i] /= alpha;
}

function premultiply(color: number[]): void {
  for (let i = 0; i < 3; i++) color[i] *= color[3];
}

/** One pass: sets `out` to the premultiplied output at the pixel whose centre is `x`, `y`. */
type Pass = (x: number, y: number, out: number[]) => void;

function pass(filter: BuiltinFilter, source: Region, box: Box): Pass {
  const [boxLeft, boxTop, boxRight, boxBottom] = box;
  // Only keeps a box of no width or height from dividing by zero, as the GPU does.
  const boxWidth = Math.max(boxRight - boxLeft, 1e-4), boxHeight = Math.max(boxBottom - boxTop, 1e-4);
  const at = (x: number, y: number, out: number[]) => {
    out.fill(0);
    source.load(Math.floor(x), Math.floor(y), 1, out);
  };
  // The average of 4×4 samples over a cell of a grid of `size` turned by `degrees` about the box's top-left.
  const cellAverage = (cellX: number, cellY: number, size: number, cos: number, sin: number, out: number[]) => {
    out.fill(0);
    for (let j = 0; j < 4; j++) {
      for (let i = 0; i < 4; i++) {
        const gridX = (cellX + (i + 0.5) / 4) * size, gridY = (cellY + (j + 0.5) / 4) * size;
        source.sample(boxLeft + cos * gridX - sin * gridY, boxTop + sin * gridX + cos * gridY, 1 / 16, out);
      }
    }
  };
  switch (filter.type) {
    case 'brightness':
    case 'contrast':
    case 'saturation':
    case 'hue':
    case 'grayscale':
    case 'invert':
    case 'tint':
    case 'vignette': {
      const apply = step(filter);
      return (x, y, out) => {
        at(x, y, out);
        unpremultiply(out);
        apply(out, (x - boxLeft) / boxWidth, (y - boxTop) / boxHeight);
        for (let i = 0; i < 3; i++) out[i] = clamp01(out[i]);
        premultiply(out);
      };
    }
    case 'pixelate': {
      const size = filter.size;
      if (size <= 1) return at;
      const cells = new Map<string, number[]>();
      return (x, y, out) => {
        const cellX = Math.floor((x - boxLeft) / size), cellY = Math.floor((y - boxTop) / size);
        const key = `${cellX},${cellY}`;
        let average = cells.get(key);
        if (!average) {
          average = [0, 0, 0, 0];
          cellAverage(cellX, cellY, size, 1, 0, average);
          cells.set(key, average);
        }
        for (let i = 0; i < 4; i++) out[i] = average[i];
      };
    }
    case 'halftone': {
      const size = Math.max(filter.size, 1e-4), angle = turn(filter.angle) * Math.PI / 180;
      const cos = Math.cos(angle), sin = Math.sin(angle);
      const cells = new Map<string, [number[], number]>();
      return (x, y, out) => {
        // The pixel in the grid's turned space.
        const localX = x - boxLeft, localY = y - boxTop;
        const gridX = cos * localX + sin * localY, gridY = -sin * localX + cos * localY;
        const cellX = Math.floor(gridX / size), cellY = Math.floor(gridY / size);
        const key = `${cellX},${cellY}`;
        let cell = cells.get(key);
        if (!cell) {
          const color = [0, 0, 0, 0];
          cellAverage(cellX, cellY, size, cos, sin, color);
          const straight = color.slice();
          unpremultiply(straight);
          const luma = clamp01(straight[0] * LR + straight[1] * LG + straight[2] * LB);
          cell = [color, size * Math.sqrt(luma / Math.PI)];
          cells.set(key, cell);
        }
        const [color, radius] = cell;
        const distance = Math.hypot(gridX - (cellX + 0.5) * size, gridY - (cellY + 0.5) * size);
        // No pixel takes more than the whole dot, however small.
        const coverage = Math.min(clamp01(radius - distance + 0.5), Math.PI * radius * radius);
        for (let i = 0; i < 4; i++) out[i] = color[i] * coverage;
      };
    }
    case 'chromaticAberration': {
      const angle = turn(filter.angle) * Math.PI / 180;
      const dx = filter.offset * Math.cos(angle), dy = filter.offset * Math.sin(angle);
      const red = [0, 0, 0, 0], green = [0, 0, 0, 0], blue = [0, 0, 0, 0];
      return (x, y, out) => {
        red.fill(0); green.fill(0); blue.fill(0);
        source.sample(x - dx, y - dy, 1, red);
        source.sample(x, y, 1, green);
        source.sample(x + dx, y + dy, 1, blue);
        out[0] = red[0]; out[1] = green[1]; out[2] = blue[2];
        out[3] = Math.max(red[3], green[3], blue[3]);
      };
    }
    case 'wave': {
      const angle = turn(filter.angle) * Math.PI / 180;
      const towardX = Math.cos(angle), towardY = Math.sin(angle);
      const wavelength = Math.max(filter.wavelength, 1e-4), phase = turn(filter.phase) / 360;
      return (x, y, out) => {
        // Reduced to one turn first, so a far pixel keeps its precision.
        const across = -towardY * (x - boxLeft) + towardX * (y - boxTop);
        const shift = filter.amplitude * Math.sin(2 * Math.PI * fract(across / wavelength + phase));
        out.fill(0);
        source.sample(x - towardX * shift, y - towardY * shift, 1, out);
      };
    }
    case 'noise': {
      const amount = clamp01(filter.amount), seed = filter.seed >>> 0;
      return (x, y, out) => {
        at(x, y, out);
        unpremultiply(out);
        const pixelX = Math.floor(x), pixelY = Math.floor(y);
        const first = random(pixelX, pixelY, seed, 0);
        for (let i = 0; i < 3; i++) {
          const grain = filter.monochrome || i === 0 ? first : random(pixelX, pixelY, seed, i);
          out[i] = clamp01(out[i] + amount * (2 * grain - 1));
        }
        premultiply(out);
      };
    }
    case 'scanlines': {
      const amount = clamp01(filter.amount), spacing = Math.max(filter.spacing, 1e-4);
      const width = clamp01(filter.thickness) * spacing;
      // The length of the bands between 0 and `t`.
      const bands = (t: number) => {
        const period = Math.floor(t / spacing);
        return period * width + Math.min(t - period * spacing, width);
      };
      return (x, y, out) => {
        at(x, y, out);
        unpremultiply(out);
        const row = y - boxTop - filter.offset;
        const scale = 1 - amount * clamp01(bands(row + 0.5) - bands(row - 0.5));
        for (let i = 0; i < 3; i++) out[i] = clamp01(out[i] * scale);
        premultiply(out);
      };
    }
    case 'zoomBlur': {
      const amount = clamp01(filter.amount);
      const centreX = boxLeft + filter.centerX * (boxRight - boxLeft);
      const centreY = boxTop + filter.centerY * (boxBottom - boxTop);
      return (x, y, out) => {
        out.fill(0);
        for (let i = 0; i < ZOOM_SAMPLES; i++) {
          const scale = 1 - amount * i / (ZOOM_SAMPLES - 1);
          source.sample(centreX + (x - centreX) * scale, centreY + (y - centreY) * scale, 1 / ZOOM_SAMPLES, out);
        }
      };
    }
    case 'chromaKey': {
      const [keyR, keyG, keyB] = hexColor(filter.color);
      const [keyCb, keyCr] = chroma(keyR, keyG, keyB);
      const tolerance = Math.max(filter.tolerance, 0), softness = Math.max(filter.softness, 1e-4);
      const spill = clamp01(filter.spill), reach = Math.hypot(keyCb, keyCr);
      return (x, y, out) => {
        at(x, y, out);
        unpremultiply(out);
        const [r, g, b] = out;
        let [cb, cr] = chroma(r, g, b);
        const t = clamp01((Math.hypot(cb - keyCb, cr - keyCr) - tolerance) / softness);
        const keep = t * t * (3 - 2 * t);
        // Takes the key's chroma out of what remains, keeping luma.
        if (reach > 1e-4) {
          const towardCb = keyCb / reach, towardCr = keyCr / reach;
          const along = Math.max(cb * towardCb + cr * towardCr, 0) * spill;
          cb -= towardCb * along;
          cr -= towardCr * along;
        }
        const luma = r * LR + g * LG + b * LB;
        out[0] = luma + 1.5748 * cr;
        out[2] = luma + 1.8556 * cb;
        out[1] = (luma - LR * out[0] - LB * out[2]) / LG;
        for (let i = 0; i < 3; i++) out[i] = clamp01(out[i]);
        out[3] *= keep;
        premultiply(out);
      };
    }
    default:
      throw new Error(`Unsupported filter: ${(filter as { type: string }).type}`);
  }
}

/** The whole pixels `box` touches, clipped to `width` by `height`; `null` when none. */
function wholePixels(box: Box, width: number, height: number): Box | null {
  const left = clamp(Math.floor(box[0]), 0, width), top = clamp(Math.floor(box[1]), 0, height);
  const right = clamp(Math.ceil(box[2]), 0, width), bottom = clamp(Math.ceil(box[3]), 0, height);
  return right > left && bottom > top ? [left, top, right, bottom] : null;
}

const expand = (box: Box, by: number): Box => [box[0] - by, box[1] - by, box[2] + by, box[3] + by];

export interface FilterOptions {
  /**
   * The box of the layer's shapes, in pixels: what grids, centres, and
   * `uv` follow. Defaults to the box of the pixels that are not transparent.
   */
  content?: Box;
  /** The scene's width and height, which bound a zoom blur's reach. Defaults to the surface's. */
  scene?: readonly [number, number];
}

/**
 * Applies the built-in `filters` in order to straight RGBA `pixels`
 * (`ImageData.data`) `width` pixels wide, as the GPU renderer does: each
 * one pass over premultiplied colors, clamped as a pass's output is,
 * writing what the layers cover grown by its padding and clearing the rest.
 * Returns the box of pixels that may have changed, or `null` when every
 * pixel is transparent.
 */
export function applyFilters(
  pixels: Uint8ClampedArray, width: number, filters: readonly BuiltinFilter[], options: FilterOptions = {},
): Box | null {
  const height = pixels.length / 4 / width;
  let left = width, top = height, right = 0, bottom = 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[(y * width + x) * 4 + 3] === 0) continue;
      if (x < left) left = x;
      if (x >= right) right = x + 1;
      if (y < top) top = y;
      if (y >= bottom) bottom = y + 1;
    }
  }
  if (right <= left) return null;
  const drawn: Box = [left, top, right, bottom];
  const box = options.content ?? drawn;
  const [sceneWidth, sceneHeight] = options.scene ?? [width, height];
  const paddings = filters.map(filter => padding(filter, Math.hypot(sceneWidth, sceneHeight)));
  // What the GPU's passes start from: the layers' bounds, 2 pixels wider
  // than their shapes for rounding.
  let written: Box = [
    Math.min(drawn[0], box[0] - 2), Math.min(drawn[1], box[1] - 2),
    Math.max(drawn[2], box[2] + 2), Math.max(drawn[3], box[3] + 2),
  ];
  // Every pass writes within this region of the surface.
  const region = wholePixels(expand(written, paddings.reduce((sum, value) => sum + value, 0)), width, height)!;
  const regionWidth = region[2] - region[0];
  let source = new Region(region);
  for (let y = region[1]; y < region[3]; y++) {
    for (let x = region[0]; x < region[2]; x++) {
      const from = (y * width + x) * 4, to = ((y - region[1]) * regionWidth + x - region[0]) * 4;
      const alpha = pixels[from + 3] / 255;
      for (let i = 0; i < 3; i++) source.data[to + i] = pixels[from + i] / 255 * alpha;
      source.data[to + 3] = alpha;
    }
  }
  let target = new Region(region);
  const out = [0, 0, 0, 0];
  filters.forEach((filter, index) => {
    written = expand(written, paddings[index]);
    const [passLeft, passTop, passRight, passBottom] = wholePixels(written, width, height)!;
    const run = pass(filter, source, box);
    target.data.fill(0);
    for (let y = passTop; y < passBottom; y++) {
      for (let x = passLeft; x < passRight; x++) {
        run(x + 0.5, y + 0.5, out);
        // A valid premultiplied color, as the shader entry clamps it.
        const alpha = clamp01(out[3]);
        const at = ((y - region[1]) * regionWidth + x - region[0]) * 4;
        for (let i = 0; i < 3; i++) target.data[at + i] = clamp(out[i], 0, alpha);
        target.data[at + 3] = alpha;
      }
    }
    [source, target] = [target, source];
  });
  const changed = wholePixels(written, width, height)!;
  for (let y = changed[1]; y < changed[3]; y++) {
    for (let x = changed[0]; x < changed[2]; x++) {
      const from = ((y - region[1]) * regionWidth + x - region[0]) * 4, to = (y * width + x) * 4;
      const alpha = source.data[from + 3];
      for (let i = 0; i < 3; i++) pixels[to + i] = alpha > 0 ? Math.round(source.data[from + i] / alpha * 255) : 0;
      pixels[to + 3] = Math.round(alpha * 255);
    }
  }
  return changed;
}
