// Custom shader effects as the scene walker accepts them. `@celesta/shader`
// builds them through `@celesta/react/internal`; the walker serializes them
// into `effects.shader` and the scene's `shaders`.

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

/** A custom shader with concrete values. Made by a shader from `@celesta/shader`. */
export class ShaderEffect {
  readonly #source: ShaderSourceHandle;
  readonly #params: readonly number[];
  readonly #padding: number;

  /** @internal */
  constructor(source: ShaderSourceHandle, params: readonly number[], padding: number) {
    this.#source = source;
    this.#params = params;
    this.#padding = padding;
  }

  /** @internal */
  static source(effect: ShaderEffect): ShaderSourceHandle {
    return effect.#source;
  }

  /** @internal */
  static serialize(effect: ShaderEffect): { id: string; params?: number[]; padding?: number } {
    return {
      id: effect.#source.id,
      ...(effect.#params.length > 0 ? { params: [...effect.#params] } : {}),
      ...(effect.#padding !== 0 ? { padding: effect.#padding } : {}),
    };
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
