/// <reference path="../wgsl.d.ts" preserve="true" />
import type { ShaderEffect } from '@celesta/react';
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
export type ShaderParamSpec = ShaderParamType | {
    readonly type: ShaderParamType;
    readonly default?: ShaderParamValue;
};
type SpecType<S> = S extends ShaderParamType ? S : S extends {
    readonly type: infer T;
} ? T : never;
type HasDefault<S> = S extends {
    readonly default: unknown;
} ? true : false;
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
    (...args: {} extends ShaderParamValues<P> ? [params?: ShaderParamValues<P>, options?: ShaderUseOptions] : [params: ShaderParamValues<P>, options?: ShaderUseOptions]): ShaderEffect;
    /** Identifies the shader's source and parameters. */
    readonly id: string;
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
export declare function defineShader<const P extends Record<string, ShaderParamSpec> = {}>(options: ShaderDefinitionOptions<P>): Shader<P>;
