// Filters as the scene walker accepts them: custom shaders, which
// `@celesta/shader` builds through `@celesta/react/internal`, and the
// built-in filters (color filters, stylize effects, and the chroma key).
// The walker serializes them into `effects.filters` and lists the custom
// shaders' sources in the scene's `shaders`.

import { isHexColor, parseHexColor } from '@celesta/math';
import type { LayerFilter } from './generated/LayerFilter';
import type { ShaderSource } from './generated/ShaderSource';
import type { ShaderParamType } from './generated/ShaderParamType';

const MAX_PARAMS = 16;

const COMPONENTS: Record<ShaderParamType, number> = { f32: 1, vec2: 2, vec3: 3, vec4: 4 };

/** A shader definition registered with `shaderSource()`. */
export class ShaderSourceHandle {
  readonly id: string;
  readonly source: ShaderSource;
  /** How many numbers one use of the shader gives. */
  readonly components: number;

  /** @internal */
  constructor(source: ShaderSource, components: number) {
    this.id = source.id;
    this.source = source;
    this.components = components;
  }
}

/** A built-in filter as the scene lists it. */
export type BuiltinFilter = Exclude<LayerFilter, { type: 'shader' }>;

/**
 * A built-in filter whose noise seed, when left out, is the frame the
 * walker serializes it at.
 */
type PendingFilter =
  | Exclude<BuiltinFilter, { type: 'noise' }>
  | (Omit<Extract<BuiltinFilter, { type: 'noise' }>, 'seed'> & { seed: number | null });

/**
 * A filter with concrete values: a custom shader from `@celesta/shader`'s
 * `defineShader()`, or a built-in filter such as `brightness(1.2)` or
 * `pixelate(8)`.
 */
export class ShaderEffect {
  readonly #source: ShaderSourceHandle | null;
  readonly #params: readonly number[];
  readonly #padding: number;
  readonly #filter: PendingFilter | null;

  /** @internal */
  constructor(source: ShaderSourceHandle | PendingFilter, params: readonly number[] = [], padding = 0) {
    const custom = source instanceof ShaderSourceHandle;
    this.#source = custom ? source : null;
    this.#filter = custom ? null : source;
    this.#params = params;
    this.#padding = padding;
  }

  /** @internal The custom shader's source; `null` for a built-in filter. */
  static source(effect: ShaderEffect): ShaderSourceHandle | null {
    return effect.#source;
  }

  /**
   * @internal `frame` is the noise seed of a noise filter that left it
   * out; `null` where the walker cannot depend on the time.
   */
  static serialize(effect: ShaderEffect, frame: number | null = null): LayerFilter {
    const filter = effect.#filter;
    if (filter?.type === 'noise' && filter.seed === null) {
      if (frame === null) throw new Error('noise inside <Static> needs a seed');
      return { ...filter, seed: frame >>> 0 };
    }
    if (filter) return { ...filter } as BuiltinFilter;
    return {
      type: 'shader',
      id: effect.#source!.id,
      ...(effect.#params.length > 0 ? { params: [...effect.#params] } : {}),
      ...(effect.#padding !== 0 ? { padding: effect.#padding } : {}),
    };
  }
}

/** `tint`: lays `color` over the pixels at `amount`, keeping their alpha. */
export interface LayerTint {
  /** `#RRGGBB` or `#RRGGBBAA`; its alpha scales `amount`. */
  color: string;
  /** 0 keeps the pixels, 1 replaces their color. Defaults to 1. */
  amount?: number;
}

/** `vignette`: darkens toward the edges of the layer's content box. */
export interface LayerVignette {
  /** How dark the corners get: 0 not at all, 1 black. */
  amount: number;
  /**
   * Where darkening starts, from 0 at the box's centre to 1 at its
   * corners. Defaults to 0.5.
   */
  radius?: number;
  /** How far beyond `radius` darkening takes to reach `amount`. Defaults to 0.5. */
  softness?: number;
}

/** The value each built-in color filter takes. */
export interface ColorFilterValues {
  /** Multiplies the color: 1 keeps it, 0 is black, above 1 brightens. */
  brightness: number;
  /** Scales the distance from mid-gray: 1 keeps it, 0 is mid-gray. */
  contrast: number;
  /** 1 keeps the color, 0 is grayscale, above 1 saturates. */
  saturation: number;
  /** Rotates the hue, in degrees. */
  hue: number;
  /** 0 keeps the color, 1 is grayscale. */
  grayscale: number;
  /** 0 keeps the color, 1 inverts it. */
  invert: number;
  tint: LayerTint;
  /** A number is the `amount`, with the default radius and softness. */
  vignette: number | LayerVignette;
}

export type ColorFilterName = keyof ColorFilterValues;

/** The built-in color filters, in the order their props apply. */
export const COLOR_FILTERS: readonly ColorFilterName[] = [
  'brightness', 'contrast', 'saturation', 'hue', 'grayscale', 'invert', 'tint', 'vignette',
];


/**
 * The built-in color filter `name` with `value`, checked; throws naming
 * the filter. Each works on the straight sRGB color, keeps alpha, and
 * clamps its result to 0–1.
 */
export function colorFilter<N extends ColorFilterName>(name: N, value: ColorFilterValues[N]): ShaderEffect;
export function colorFilter(name: ColorFilterName, value: unknown): ShaderEffect {
  const number = (input: unknown, label: string, min = -Infinity, max = Infinity): number => {
    if (typeof input !== 'number' || !Number.isFinite(input) || input < min || input > max) {
      const range = max === Infinity
        ? (min === -Infinity ? '' : ` of at least ${min}`)
        : ` between ${min} and ${max}`;
      throw new Error(`${label} must be a finite number${range}`);
    }
    return input;
  };
  const object = (input: unknown, label: string): Record<string, unknown> => {
    if (input === null || typeof input !== 'object' || Array.isArray(input)) {
      throw new Error(`${label} must be an object`);
    }
    return input as Record<string, unknown>;
  };
  switch (name) {
    case 'brightness':
    case 'contrast':
    case 'saturation':
      return new ShaderEffect({ type: name, amount: number(value, name, 0) });
    case 'hue':
      return new ShaderEffect({ type: 'hue', degrees: number(value, 'hue') });
    case 'grayscale':
    case 'invert':
      return new ShaderEffect({ type: name, amount: number(value, name, 0, 1) });
    case 'tint': {
      const tint = object(value, 'tint');
      if (!isHexColor(tint.color)) {
        throw new Error('tint.color must be a #RRGGBB or #RRGGBBAA color');
      }
      const amount = tint.amount === undefined ? 1 : number(tint.amount, 'tint.amount', 0, 1);
      return new ShaderEffect({ type: 'tint', color: tint.color, amount });
    }
    case 'vignette': {
      const vignette = typeof value === 'number' ? { amount: value } : object(value, 'vignette');
      const label = typeof value === 'number' ? 'vignette' : 'vignette.amount';
      return new ShaderEffect({
        type: 'vignette',
        amount: number(vignette.amount, label, 0, 1),
        radius: vignette.radius === undefined ? 0.5 : number(vignette.radius, 'vignette.radius', 0),
        softness: vignette.softness === undefined ? 0.5 : number(vignette.softness, 'vignette.softness', 0),
      });
    }
    default:
      throw new Error(`unknown color filter ${JSON.stringify(name)}`);
  }
}

/** `pixelate`: averages square cells. */
export interface LayerPixelate {
  /** The cells' side, in output pixels, from the content box's top-left. 1 or less keeps the pixels. */
  size: number;
}

/** `halftone`: dots of each cell's color, sized by its brightness. */
export interface LayerHalftone {
  /** The grid's spacing, in output pixels. 0 keeps the pixels. */
  size: number;
  /** The grid's turn about the content box's top-left, in degrees. Defaults to 45. */
  angle?: number;
}

/** `chromaticAberration`: moves red and blue apart. */
export interface LayerChromaticAberration {
  /** How far red moves toward `angle`, and blue the other way, in output pixels. */
  offset: number;
  /** The direction red moves, in degrees: 0 is right, 90 down. Defaults to 0. */
  angle?: number;
}

/** `wave`: sways the pixels along a sine. */
export interface LayerWave {
  /** How far the pixels move, in output pixels. */
  amplitude: number;
  /** The sine's length across the direction they move, in output pixels. */
  wavelength: number;
  /** Shifts the sine, in degrees; animate it to make the wave travel. Defaults to 0. */
  phase?: number;
  /** The direction the pixels move, in degrees: 0 sways rows sideways. Defaults to 0. */
  angle?: number;
}

/** `noise`: grain over the color. */
export interface LayerNoise {
  /** How far each channel can move, 0–1. */
  amount: number;
  /**
   * An integer choosing the grain. Defaults to the layer's current frame
   * (`useCurrentFrame()`), so the grain changes every frame; give one to
   * hold it still.
   */
  seed?: number;
  /** One value for all three channels. Defaults to true. */
  monochrome?: boolean;
}

/** `scanlines`: darkened horizontal bands. */
export interface LayerScanlines {
  /** How dark the bands are: 0 not at all, 1 black. */
  amount: number;
  /** The distance from one band to the next, in output pixels. Defaults to 4. */
  spacing?: number;
  /** The bands' thickness as a part of `spacing`, 0–1. Defaults to 0.5. */
  thickness?: number;
  /** Moves the bands down from the content box's top, in output pixels. Defaults to 0. */
  offset?: number;
}

/** `zoomBlur`: blurs toward a centre, as a fast zoom would. */
export interface LayerZoomBlur {
  /** How much of each pixel's distance to the centre the blur spans, 0–1. */
  amount: number;
  /** The centre across the content box, 0 at its left and 1 at its right. Defaults to 0.5. */
  centerX?: number;
  /** The centre down the content box, 0 at its top and 1 at its bottom. Defaults to 0.5. */
  centerY?: number;
}

/** `chromaKey`: makes the colors near a key color transparent. */
export interface LayerChromaKey {
  /** The key color, `#RRGGBB`. */
  color: string;
  /**
   * How near the key a color's chroma (its BT.709 CbCr) must be to become
   * fully transparent. Defaults to 0.2.
   */
  tolerance?: number;
  /** How far beyond `tolerance` colors fade from transparent to opaque. Defaults to 0.1. */
  softness?: number;
  /** How much of the key's color cast to take out of what remains, 0–1. Defaults to 0.5. */
  spill?: number;
}

/** The value each built-in stylize effect, and the chroma key, takes. */
export interface EffectFilterValues {
  /** A number is the `size`. */
  pixelate: number | LayerPixelate;
  /** A number is the `size`. */
  halftone: number | LayerHalftone;
  /** A number is the `offset`. */
  chromaticAberration: number | LayerChromaticAberration;
  wave: LayerWave;
  /** A number is the `amount`. */
  noise: number | LayerNoise;
  /** A number is the `amount`. */
  scanlines: number | LayerScanlines;
  /** A number is the `amount`. */
  zoomBlur: number | LayerZoomBlur;
  /** A string is the `color`. */
  chromaKey: string | LayerChromaKey;
}

export type EffectFilterName = keyof EffectFilterValues;

/** Every built-in filter's value. */
export type BuiltinFilterValues = ColorFilterValues & EffectFilterValues;

export type BuiltinFilterName = keyof BuiltinFilterValues;

/** The built-in filters, in the order their props apply. */
export const FILTER_PROPS: readonly BuiltinFilterName[] = [
  'chromaKey', 'pixelate', 'wave', 'zoomBlur', 'chromaticAberration',
  ...COLOR_FILTERS,
  'halftone', 'noise', 'scanlines',
];

/**
 * The built-in filter `name` with `value`, checked; throws naming the
 * filter. Spatial values are output pixels and angles degrees.
 */
export function builtinFilter<N extends BuiltinFilterName>(name: N, value: BuiltinFilterValues[N]): ShaderEffect;
export function builtinFilter(name: BuiltinFilterName, value: unknown): ShaderEffect {
  if ((COLOR_FILTERS as readonly string[]).includes(name)) return colorFilter(name as ColorFilterName, value as never);
  // A number or a string stands for the first field.
  const fields = (first: string): Record<string, unknown> => {
    if (typeof value === 'number' || typeof value === 'string') return { [first]: value };
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      throw new Error(`${name} must be an object`);
    }
    return value as Record<string, unknown>;
  };
  const scalar = typeof value === 'number' || typeof value === 'string';
  const label = (field: string) => (scalar ? name : `${name}.${field}`);
  const number = (
    input: unknown, field: string, fallback?: number, min = -Infinity, max = Infinity, above = false,
  ): number => {
    if (input === undefined && fallback !== undefined) return fallback;
    if (typeof input !== 'number' || !Number.isFinite(input) || input < min || input > max || (above && input <= min)) {
      const range = max !== Infinity
        ? ` between ${min} and ${max}`
        : above ? ` above ${min}` : min !== -Infinity ? ` of at least ${min}` : '';
      throw new Error(`${label(field)} must be a finite number${range}`);
    }
    return input;
  };
  switch (name) {
    case 'pixelate': {
      const pixelate = fields('size');
      return new ShaderEffect({ type: 'pixelate', size: number(pixelate.size, 'size', undefined, 0) });
    }
    case 'halftone': {
      const halftone = fields('size');
      return new ShaderEffect({
        type: 'halftone',
        size: number(halftone.size, 'size', undefined, 0),
        angle: number(halftone.angle, 'angle', 45),
      });
    }
    case 'chromaticAberration': {
      const shift = fields('offset');
      return new ShaderEffect({
        type: 'chromaticAberration',
        offset: number(shift.offset, 'offset'),
        angle: number(shift.angle, 'angle', 0),
      });
    }
    case 'wave': {
      if (value === null || typeof value !== 'object' || Array.isArray(value)) {
        throw new Error('wave must be an object with amplitude and wavelength');
      }
      const wave = value as Record<string, unknown>;
      return new ShaderEffect({
        type: 'wave',
        amplitude: number(wave.amplitude, 'amplitude'),
        wavelength: number(wave.wavelength, 'wavelength', undefined, 0, Infinity, true),
        phase: number(wave.phase, 'phase', 0),
        angle: number(wave.angle, 'angle', 0),
      });
    }
    case 'noise': {
      const noise = fields('amount');
      if (noise.seed !== undefined && !Number.isSafeInteger(noise.seed)) {
        throw new Error('noise.seed must be an integer');
      }
      if (noise.monochrome !== undefined && typeof noise.monochrome !== 'boolean') {
        throw new Error('noise.monochrome must be a boolean');
      }
      return new ShaderEffect({
        type: 'noise',
        amount: number(noise.amount, 'amount', undefined, 0, 1),
        seed: noise.seed === undefined ? null : (noise.seed as number) >>> 0,
        monochrome: noise.monochrome !== false,
      });
    }
    case 'scanlines': {
      const scanlines = fields('amount');
      return new ShaderEffect({
        type: 'scanlines',
        amount: number(scanlines.amount, 'amount', undefined, 0, 1),
        spacing: number(scanlines.spacing, 'spacing', 4, 0, Infinity, true),
        thickness: number(scanlines.thickness, 'thickness', 0.5, 0, 1),
        offset: number(scanlines.offset, 'offset', 0),
      });
    }
    case 'zoomBlur': {
      const zoom = fields('amount');
      return new ShaderEffect({
        type: 'zoomBlur',
        amount: number(zoom.amount, 'amount', undefined, 0, 1),
        centerX: number(zoom.centerX, 'centerX', 0.5),
        centerY: number(zoom.centerY, 'centerY', 0.5),
      });
    }
    case 'chromaKey': {
      const key = fields('color');
      if (!isHexColor(key.color) || key.color.length !== 7) {
        throw new Error(`${label('color')} must be a #RRGGBB color`);
      }
      return new ShaderEffect({
        type: 'chromaKey',
        color: key.color,
        tolerance: number(key.tolerance, 'tolerance', 0.2, 0),
        softness: number(key.softness, 'softness', 0.1, 0),
        spill: number(key.spill, 'spill', 0.5, 0, 1),
      });
    }
    default:
      throw new Error(`unknown filter ${JSON.stringify(name)}`);
  }
}

/** Whether the built-in filter leaves every pixel as it was. */
export function isIdentityFilter(filter: BuiltinFilter): boolean {
  switch (filter.type) {
    case 'brightness':
    case 'contrast':
    case 'saturation':
      return filter.amount === 1;
    case 'hue':
      return filter.degrees % 360 === 0;
    case 'tint':
      return filter.amount === 0 || parseHexColor(filter.color)?.[3] === 0;
    case 'pixelate':
      return filter.size <= 1;
    case 'halftone':
      return filter.size === 0;
    case 'chromaticAberration':
      return filter.offset === 0;
    case 'wave':
      return filter.amplitude === 0;
    case 'chromaKey':
      return false;
    default:
      return filter.amount === 0;
  }
}

/**
 * 64-bit FNV-1a over UTF-16 code units, as 16 hex digits. Not
 * cryptographic: the renderer compares the source before reusing a compiled
 * shader, so a collision costs a recompile, not a wrong picture.
 */
function fnv1a64(text: string): string {
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  let hash = 0xcbf29ce484222325n;
  for (let index = 0; index < text.length; index++) {
    hash = ((hash ^ BigInt(text.charCodeAt(index))) * prime) & mask;
  }
  return hash.toString(16).padStart(16, '0');
}

/** `value` as a flat string; see `flatString` in render.ts. */
function flat(value: string): string {
  return value.length < 13 ? value : JSON.parse(JSON.stringify(value));
}

/**
 * Registers a shader definition once: checks its shape, flattens its strings
 * for the per-frame serialization, and computes its id from the source and
 * the parameters' names and types.
 */
export function shaderSource(definition: {
  name?: string;
  wgsl: string;
  params: readonly { name: string; type: ShaderParamType }[];
}): ShaderSourceHandle {
  const { name, wgsl, params } = definition;
  if (typeof wgsl !== 'string' || wgsl.length === 0) {
    throw new Error('shaderSource: wgsl must be a non-empty string');
  }
  if (name !== undefined && typeof name !== 'string') {
    throw new Error('shaderSource: name must be a string');
  }
  if (!Array.isArray(params) || params.length > MAX_PARAMS) {
    throw new Error(`shaderSource: params must be an array of at most ${MAX_PARAMS} parameters`);
  }
  let components = 0;
  for (const param of params) {
    if (typeof param?.name !== 'string' || !Object.prototype.hasOwnProperty.call(COMPONENTS, param.type)) {
      throw new Error('shaderSource: each parameter needs a name and a type of f32, vec2, vec3, or vec4');
    }
    components += COMPONENTS[param.type as ShaderParamType];
  }
  // JSON of an array keeps the encoding unambiguous for any name.
  const id = flat(fnv1a64(JSON.stringify([wgsl, params.map((param) => [param.name, param.type])])));
  return new ShaderSourceHandle(
    {
      id,
      ...(name !== undefined ? { name: flat(name) } : {}),
      wgsl: flat(wgsl),
      ...(params.length > 0
        ? { params: params.map((param) => ({ name: flat(param.name), type: param.type })) }
        : {}),
    },
    components,
  );
}

/** One use of a shader: every parameter's components in declared order, and the padding. */
export function shaderEffect(
  source: ShaderSourceHandle,
  params: readonly number[],
  padding: number,
): ShaderEffect {
  if (!(source instanceof ShaderSourceHandle)) {
    throw new Error('shaderEffect: source must come from shaderSource()');
  }
  if (!Array.isArray(params) || params.length !== source.components) {
    throw new Error(`shaderEffect: expected ${source.components} parameter values`);
  }
  // A dense copy: `every` would skip the holes of a sparse array.
  const values = Array.from(params);
  if (!values.every((value) => typeof value === 'number' && Number.isFinite(value))) {
    throw new Error('shaderEffect: parameter values must be finite numbers');
  }
  if (typeof padding !== 'number' || !Number.isFinite(padding) || padding < 0) {
    throw new Error('shaderEffect: padding must be a finite number of at least 0');
  }
  return new ShaderEffect(source, values, padding);
}
