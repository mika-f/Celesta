/// <reference path="../wgsl.d.ts" preserve="true" />

import type { ShaderEffect } from '@celesta/react';
import { shaderEffect, shaderSource } from '@celesta/react/internal';

export type { ShaderEffect } from '@celesta/react';

/** The type of a shader parameter, and the WGSL type it is declared as. */
export type ShaderParamType = 'f32' | 'vec2' | 'vec3' | 'vec4' | 'color';

/** What each parameter type takes in React. */
export interface ShaderParamValueOf {
  f32: number;
  vec2: readonly [number, number];
  vec3: readonly [number, number, number];
  vec4: readonly [number, number, number, number];
  /** `#RRGGBB` or `#RRGGBBAA`; the shader gets straight RGBA in 0–1. */
  color: string;
}

export type ShaderParamValue = ShaderParamValueOf[ShaderParamType];

/** A parameter's type, or its type and the value used when a use omits it. */
export type ShaderParamSpec =
  | ShaderParamType
  | { readonly type: ShaderParamType; readonly default?: ShaderParamValue };

type SpecType<S> = S extends ShaderParamType ? S : S extends { readonly type: infer T } ? T : never;
type HasDefault<S> = S extends { readonly default: unknown } ? true : false;

/** The values one use of a shader takes: those without a default are required. */
export type ShaderParamValues<P> = {
  -readonly [K in keyof P as HasDefault<P[K]> extends true ? never : K]: ShaderParamValueOf[SpecType<P[K]> & ShaderParamType];
} & {
  -readonly [K in keyof P as HasDefault<P[K]> extends true ? K : never]?: ShaderParamValueOf[SpecType<P[K]> & ShaderParamType];
};

export interface ShaderDefinitionOptions<P extends Record<string, ShaderParamSpec>> {
  /** The `effect` function and any helpers it uses, in WGSL. */
  wgsl: string;
  /** Names the shader in error messages. */
  name?: string;
  /** Up to 16 parameters, in the order they are declared to WGSL. */
  params?: P;
  /** Output pixels the shader may write beyond the content box. Defaults to 0. */
  padding?: number;
}

export interface ShaderUseOptions {
  /** Overrides the definition's padding for this use. */
  padding?: number;
}

/** A defined shader: call it with its values to get the `shader` prop. */
export interface Shader<P> {
  (
    ...args: {} extends ShaderParamValues<P>
      ? [params?: ShaderParamValues<P>, options?: ShaderUseOptions]
      : [params: ShaderParamValues<P>, options?: ShaderUseOptions]
  ): ShaderEffect;
  /** Identifies the shader's source and parameters. */
  readonly id: string;
}

const MAX_PARAMS = 16;
const NAME = /^[A-Za-z][A-Za-z0-9_]*$/;
const COLOR = /^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})?$/;
const COMPONENTS: Record<Exclude<ShaderParamType, 'color'>, number> = { f32: 1, vec2: 2, vec3: 3, vec4: 4 };

function isType(value: unknown): value is ShaderParamType {
  return value === 'color' || Object.prototype.hasOwnProperty.call(COMPONENTS, value as string);
}

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/** `value`'s numbers as a parameter of `type`; throws naming `label`. */
function pack(type: ShaderParamType, value: unknown, label: string): number[] {
  if (type === 'color') {
    const match = typeof value === 'string' ? COLOR.exec(value) : null;
    if (!match) throw new Error(`${label} must be a #RRGGBB or #RRGGBBAA color`);
    return [match[1], match[2], match[3], match[4] ?? 'ff'].map((hex) => parseInt(hex!, 16) / 255);
  }
  if (type === 'f32') {
    if (!finite(value)) throw new Error(`${label} must be a finite number`);
    return [value];
  }
  const length = COMPONENTS[type];
  if (!Array.isArray(value) || value.length !== length || !value.every(finite)) {
    throw new Error(`${label} must be an array of ${length} finite numbers`);
  }
  return [...value];
}

/**
 * Defines a custom WGSL filter. Call the result with the parameters' values
 * and pass that to a layer's `shader` prop:
 *
 * ```tsx
 * const ripple = defineShader({ name: 'ripple', wgsl, params: { time: 'f32' }, padding: 8 });
 * <Group shader={ripple({ time: frame / fps })}>…</Group>
 * ```
 *
 * `wgsl` defines `fn effect(input: EffectInput) -> vec4f`, which returns
 * each output pixel as premultiplied RGBA; see the Celesta docs for the
 * prelude it can use.
 */
export function defineShader<const P extends Record<string, ShaderParamSpec> = {}>(
  options: ShaderDefinitionOptions<P>,
): Shader<P> {
  if (options === null || typeof options !== 'object') {
    throw new Error('defineShader expects an options object');
  }
  const { wgsl, name, params = {} as P, padding = 0 } = options;
  const prefix = typeof name === 'string' ? `shader ${name}` : 'shader';
  const fail = (message: string) => new Error(`${prefix}: ${message}`);
  if (name !== undefined && (typeof name !== 'string' || name.length === 0)) {
    throw new Error('defineShader: name must be a non-empty string');
  }
  if (typeof wgsl !== 'string' || wgsl.trim().length === 0) {
    throw fail('wgsl must be a non-empty string');
  }
  if (!finite(padding) || padding < 0) {
    throw fail('padding must be a finite number of at least 0');
  }
  if (params === null || typeof params !== 'object' || Array.isArray(params)) {
    throw fail('params must be an object of parameter types');
  }
  const entries = Object.entries(params as Record<string, ShaderParamSpec>);
  if (entries.length > MAX_PARAMS) {
    throw fail(`declares ${entries.length} parameters; the most is ${MAX_PARAMS}`);
  }
  const declared = entries.map(([paramName, spec]) => {
    if (!NAME.test(paramName) || paramName.startsWith('celesta')) {
      throw fail(`parameter name ${JSON.stringify(paramName)} must be letters, digits, and underscores, start with a letter, and not start with "celesta"`);
    }
    const type = typeof spec === 'string' ? spec : spec?.type;
    if (!isType(type)) {
      throw fail(`${paramName} must have a type of f32, vec2, vec3, vec4, or color`);
    }
    const fallback = typeof spec === 'object' && spec.default !== undefined
      ? pack(type, spec.default, `${prefix}: the default of ${paramName}`)
      : undefined;
    return { name: paramName, type, fallback };
  });
  const source = shaderSource({
    ...(name !== undefined ? { name } : {}),
    wgsl,
    params: declared.map((param) => ({ name: param.name, type: param.type === 'color' ? 'vec4' : param.type })),
  });

  const shader = (values: Record<string, unknown> = {}, use: ShaderUseOptions = {}): ShaderEffect => {
    if (values === null || typeof values !== 'object' || Array.isArray(values)) {
      throw fail('expects an object of parameter values');
    }
    for (const key of Object.keys(values)) {
      if (!declared.some((param) => param.name === key)) {
        throw fail(`has no parameter ${JSON.stringify(key)}`);
      }
    }
    const packed: number[] = [];
    for (const param of declared) {
      const value = values[param.name];
      if (value === undefined) {
        if (!param.fallback) throw fail(`${param.name} is required`);
        packed.push(...param.fallback);
      } else {
        packed.push(...pack(param.type, value, `${prefix}: ${param.name}`));
      }
    }
    const usePadding = use?.padding ?? padding;
    if (!finite(usePadding) || usePadding < 0) {
      throw fail('padding must be a finite number of at least 0');
    }
    return shaderEffect(source, packed, usePadding);
  };
  Object.defineProperty(shader, 'id', { value: source.id, enumerable: true });
  return shader as unknown as Shader<P>;
}
