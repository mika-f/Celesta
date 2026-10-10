"use strict";
/// <reference path="../wgsl.d.ts" preserve="true" />
Object.defineProperty(exports, "__esModule", { value: true });
exports.defineShader = defineShader;
const internal_1 = require("@celesta/react/internal");
const MAX_PARAMS = 16;
const NAME = /^[A-Za-z][A-Za-z0-9_]*$/;
const COLOR = /^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})?$/;
const COMPONENTS = { f32: 1, vec2: 2, vec3: 3, vec4: 4 };
function isType(value) {
    return value === 'color' || Object.prototype.hasOwnProperty.call(COMPONENTS, value);
}
function finite(value) {
    return typeof value === 'number' && Number.isFinite(value);
}
/** `value`'s numbers as a parameter of `type`; throws naming `label`. */
function pack(type, value, label) {
    if (type === 'color') {
        const match = typeof value === 'string' ? COLOR.exec(value) : null;
        if (!match)
            throw new Error(`${label} must be a #RRGGBB or #RRGGBBAA color`);
        return [match[1], match[2], match[3], match[4] ?? 'ff'].map((hex) => parseInt(hex, 16) / 255);
    }
    if (type === 'f32') {
        if (!finite(value))
            throw new Error(`${label} must be a finite number`);
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
function defineShader(options) {
    if (options === null || typeof options !== 'object') {
        throw new Error('defineShader expects an options object');
    }
    const { wgsl, name, params = {}, padding = 0 } = options;
    const prefix = typeof name === 'string' ? `shader ${name}` : 'shader';
    const fail = (message) => new Error(`${prefix}: ${message}`);
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
    const entries = Object.entries(params);
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
    const source = (0, internal_1.shaderSource)({
        ...(name !== undefined ? { name } : {}),
        wgsl,
        params: declared.map((param) => ({ name: param.name, type: param.type === 'color' ? 'vec4' : param.type })),
    });
    const shader = (values = {}, use = {}) => {
        if (values === null || typeof values !== 'object' || Array.isArray(values)) {
            throw fail('expects an object of parameter values');
        }
        for (const key of Object.keys(values)) {
            if (!declared.some((param) => param.name === key)) {
                throw fail(`has no parameter ${JSON.stringify(key)}`);
            }
        }
        const packed = [];
        for (const param of declared) {
            const value = values[param.name];
            if (value === undefined) {
                if (!param.fallback)
                    throw fail(`${param.name} is required`);
                packed.push(...param.fallback);
            }
            else {
                packed.push(...pack(param.type, value, `${prefix}: ${param.name}`));
            }
        }
        const usePadding = use?.padding ?? padding;
        if (!finite(usePadding) || usePadding < 0) {
            throw fail('padding must be a finite number of at least 0');
        }
        return (0, internal_1.shaderEffect)(source, packed, usePadding);
    };
    Object.defineProperty(shader, 'id', { value: source.id, enumerable: true });
    return shader;
}
