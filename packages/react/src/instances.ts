// `<Instances>`: many particles of one shape as one layer. Its columns travel
// as base64 of their raw bytes, so a frame of 100,000 particles neither
// formats nor parses a number per particle.

import { isHexColor } from '@celesta/math';

import type { LayerContent, ResolvedAsset } from './scene';

type InstancesContent = Extract<LayerContent, { type: 'instances' }>;

// The columns are sent in the host's byte order, which Rust reads as
// little-endian.
const littleEndian = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;

function fail(message: string): never {
  throw new Error(`<Instances> ${message}`);
}

function isColumn(value: unknown): value is ArrayLike<number> {
  return ArrayBuffer.isView(value) || Array.isArray(value);
}

// Bytes per base64 piece: a whole number of 3-byte groups, so the pieces
// join into the base64 of the whole, and short enough (512 KiB of text) that
// Node returns each as a flat string rather than an external one, which
// would send the frame's JSON.stringify down its slow path (see
// docs/performance/react-scene-serialization.md). `join` copies the pieces
// into one flat string.
const BASE64_PIECE = 3 * 0x2_0000;

function base64(view: ArrayBufferView): string {
  if (typeof Buffer !== 'undefined') {
    const bytes = Buffer.from(view.buffer, view.byteOffset, view.byteLength);
    if (bytes.length <= BASE64_PIECE) return bytes.toString('base64');
    const pieces: string[] = [];
    for (let start = 0; start < bytes.length; start += BASE64_PIECE) {
      pieces.push(bytes.toString('base64', start, start + BASE64_PIECE));
    }
    return pieces.join('');
  }
  // The browser runtime has no Buffer.
  const bytes = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  let binary = '';
  for (let start = 0; start < bytes.length; start += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(start, start + 0x8000));
  }
  return btoa(binary);
}

/** `value` as base64 of `count` little-endian f32s. */
function floats(name: string, value: unknown, count: number): string {
  if (!isColumn(value)) fail(`${name} must be a Float32Array or an array of numbers`);
  if (value.length !== count) fail(`${name} has ${value.length} values; count is ${count}`);
  return base64(value instanceof Float32Array ? value : Float32Array.from(value));
}

/** A shared size (a number) or a per-particle column; `undefined` when absent. */
function size(name: string, value: unknown, count: number): number | string | undefined {
  if (value === undefined) return undefined;
  if (typeof value === 'number') {
    if (!Number.isFinite(value) || value < 0) fail(`${name} must be a finite number of zero or more`);
    return value;
  }
  return floats(name, value, count);
}

/** The scene content of an `<Instances>` host element's props. */
export function instancesContent(
  props: Record<string, unknown>,
  resolveAsset: (src: unknown) => ResolvedAsset,
): LayerContent {
  if (!littleEndian) fail('requires a little-endian host');
  const { count } = props;
  if (typeof count !== 'number' || !Number.isInteger(count) || count < 0 || count > 0xffff_ffff) {
    fail('count must be a whole number of zero or more');
  }
  let shape: InstancesContent['shape'];
  if (props.src !== undefined) {
    shape = { type: 'image', asset: resolveAsset(props.src) };
  } else if (props.shape === undefined || props.shape === 'rect') {
    const cornerRadius = props.cornerRadius ?? 0;
    if (typeof cornerRadius !== 'number' || !Number.isFinite(cornerRadius)) {
      fail('cornerRadius must be a finite number');
    }
    shape = { type: 'rect', cornerRadius };
  } else if (props.shape === 'circle') {
    shape = { type: 'circle' };
  } else {
    fail(`shape must be "rect" or "circle", not ${JSON.stringify(props.shape)}`);
  }
  const width = size('width', props.width, count);
  const height = size('height', props.height, count);
  if (shape.type !== 'image') {
    if (width === undefined) fail('width is required for a rect or circle');
    if (height === undefined) fail('height is required for a rect or circle');
  }
  if (props.fill !== undefined && !isHexColor(props.fill)) fail('fill must be a #RRGGBB or #RRGGBBAA color');
  let colors: string | undefined;
  if (props.color !== undefined) {
    const { color } = props;
    if (!isColumn(color)) fail('color must be a Uint8Array or an array of numbers');
    if (color.length !== 4 * count) fail(`color has ${color.length} values; count ${count} needs ${4 * count}`);
    colors = base64(color instanceof Uint8Array ? color : Uint8Array.from(color));
  }
  return {
    type: 'instances',
    count,
    shape,
    ...(typeof width === 'number' ? { width } : {}),
    ...(typeof height === 'number' ? { height } : {}),
    ...(typeof props.fill === 'string' ? { color: props.fill } : {}),
    x: floats('x', props.x, count),
    y: floats('y', props.y, count),
    ...(typeof width === 'string' ? { widths: width } : {}),
    ...(typeof height === 'string' ? { heights: height } : {}),
    ...(props.rotation === undefined ? {} : { rotation: floats('rotation', props.rotation, count) }),
    ...(colors === undefined ? {} : { colors }),
  };
}
