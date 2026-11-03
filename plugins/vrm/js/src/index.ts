// The VRM plugin's components, imported by entries as `@celesta-plugin/vrm`.
// `<Vrm>` puts one layer in the scene: plugin content for plugin `vrm`, which
// the plugin's process (plugins/vrm) turns into a 3D scene of the avatar.
// The layer is as many pixels as `width` and `height` say (default
// 1920x1080); Celesta draws the scene at the layer's density, so a smaller
// layer scaled up is not sharper than this one.
//
// Like everything in a composition, the layer depends only on the frame:
// blinking and lip sync come from `@celesta/character`, and every value the
// plugin gets is in `vrmData`, a pure function of the frame.

import * as React from 'react';

import { blinkPhase } from '@celesta/character';
import type { BlinkTiming, LipSyncTrack, MouthShape } from '@celesta/character';
import { useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { AssetInput, CommonProps, JsonValue, LayerContent } from '@celesta/react';
import { registerHostElement } from '@celesta/react/internal';
import type { HostNode, HostWalk } from '@celesta/react/internal';

/** VRM 1.0's mouth expressions, from the vowels a, i, u, e, o. */
export type Vowel = 'aa' | 'ih' | 'ou' | 'ee' | 'oh';

/** A camera placed in the avatar's space: metres, +Y up, the avatar facing +Z. */
export interface VrmCamera {
  position: [number, number, number];
  /** The point it looks at; not `position`. */
  target: [number, number, number];
  /** The vertical field of view in degrees. Default 30. */
  fov?: number;
  /** Default 0.01. */
  near?: number;
  /** Default 100. */
  far?: number;
}

/** Lights in place of the plugin's reference light. */
export interface VrmLights {
  /** Up to four; a fifth throws. `direction` is where the light travels. */
  directional?: {
    direction: [number, number, number];
    /** `#rgb` or `#rrggbb` (sRGB). Default `#ffffff`. */
    color?: string;
    /** Default 1. */
    intensity?: number;
  }[];
  /** `#rgb` or `#rrggbb` (sRGB). Default black. */
  ambient?: string;
}

export interface VrmProps extends CommonProps {
  /** The `.vrm` (VRM 1.0). */
  src: AssetInput;
  /** The layer's width in pixels, from 1 to 16384. Default 1920, or from `height` and `aspect`. */
  width?: number;
  /** The layer's height in pixels, from 1 to 16384. Default 1080, or from `width` and `aspect`. */
  height?: number;
  /** Width over height, filling whichever of `width` and `height` is missing. Default 16:9. */
  aspect?: number;
  /**
   * Expression weights from 0 to 1, by preset (`happy`, `blink`, `aa`, …)
   * or custom name. They win over blinking and the mouth.
   */
  expressions?: Record<string, number>;
  /** The mouth: a vowel shape ('a' sets `aa` to 1), or the vowel weights. */
  mouth?: MouthShape | Partial<Record<Vowel, number>>;
  /** A lip-sync track from `loadLipSync()`, used when `mouth` is not given. */
  lipSync?: LipSyncTrack;
  /**
   * Frames over which a lip-sync track's shapes blend into each other: each
   * frame mixes the shapes of this many frames before it. Default 2; 0 snaps.
   */
  mouthSmoothing?: number;
  /** Blinks, seeded as `<CharacterView>` portraits do. `false` never blinks. Default `{}`. */
  blink?: BlinkTiming | false;
  /** Where the eyes look: the camera, a point in the avatar's space, or angles in degrees. */
  lookAt?: 'camera' | [number, number, number] | { yaw: number; pitch: number };
  /** A framing of the humanoid, or a camera of its own. Default `'full'`. */
  camera?: 'full' | 'bust' | 'face' | VrmCamera;
  /** Default: the plugin's reference light. */
  lights?: VrmLights;
}

/** The plugin's directional light, in linear colour. */
export interface VrmDirectionalLight {
  direction: [number, number, number];
  color: [number, number, number];
  intensity: number;
}

/** What the layer sends the plugin each frame (the plugin's `PerformanceData`). */
export interface VrmData {
  /** The layer's size in pixels. */
  canvas: [number, number];
  expressions?: Record<string, number>;
  lookAt?: VrmProps['lookAt'];
  camera?: VrmProps['camera'];
  lights?: { directional: VrmDirectionalLight[]; ambient?: [number, number, number] };
}

const ELEMENT = 'celesta-plugin-vrm';

const VOWELS: Record<MouthShape, Vowel | undefined> = {
  closed: undefined,
  a: 'aa',
  i: 'ih',
  u: 'ou',
  e: 'ee',
  o: 'oh',
};

const BLINK = { open: 0, half: 0.5, closed: 1 } as const;

/** The props `<Vrm>` turns into data rather than passing to its element. */
const DATA_PROPS = [
  'width',
  'height',
  'aspect',
  'expressions',
  'mouth',
  'lipSync',
  'mouthSmoothing',
  'blink',
  'lookAt',
  'camera',
  'lights',
] as const;

export function Vrm(props: VrmProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return React.createElement(ELEMENT, vrmElementProps(props, frame, fps));
}

/** The host element's props: the common ones and `src`, the time, and `vrmData`. */
export function vrmElementProps(props: VrmProps, frame: number, fps: number): Record<string, unknown> {
  const rest: Record<string, unknown> = { ...props };
  for (const key of DATA_PROPS) delete rest[key];
  return { ...rest, time: frame / fps, data: vrmData(props, frame, fps) };
}

/** What the layer sends on `frame` at `fps`: a pure function of them. */
export function vrmData(props: VrmProps, frame: number, fps: number): VrmData {
  const { mouthSmoothing } = props;
  if (mouthSmoothing !== undefined && !(Number.isFinite(mouthSmoothing) && mouthSmoothing >= 0)) {
    throw new Error('Vrm mouthSmoothing is a number of frames, 0 or more');
  }
  const expressions: Record<string, number> = {};
  if (props.blink !== false) {
    expressions.blink = BLINK[blinkPhase(frame, fps, props.blink ?? {})];
  }
  Object.assign(expressions, mouthExpressions(props, frame, fps), props.expressions);
  return {
    canvas: canvas(props),
    ...(Object.keys(expressions).length ? { expressions } : {}),
    ...(props.lookAt !== undefined ? { lookAt: props.lookAt } : {}),
    ...(props.camera !== undefined ? { camera: props.camera } : {}),
    ...(props.lights !== undefined ? { lights: lights(props.lights) } : {}),
  };
}

/**
 * The longest side of a layer's canvas Celesta draws, in pixels
 * (`celesta_plugin::scene::MAX_SCENE_CANVAS_SIDE`); the shortest is 1.
 */
const MAX_CANVAS_SIDE = 16384;

/**
 * The most directional lights a frame may have (Celesta's renderer refuses
 * a fifth).
 */
const MAX_DIRECTIONAL_LIGHTS = 4;

/**
 * `[width, height]`: as given, the missing one from `aspect`, else
 * 1920x1080. Each side, given or computed, must be one Celesta draws, so a
 * mistake throws here rather than failing every frame.
 */
function canvas(props: VrmProps): [number, number] {
  const size = canvasSize(props);
  if (!size.every((side) => side >= 1 && side <= MAX_CANVAS_SIDE)) {
    throw new Error(`Vrm width and height are from 1 to ${MAX_CANVAS_SIDE} pixels (got [${size.join(', ')}])`);
  }
  return size;
}

function canvasSize({ width, height, aspect }: VrmProps): [number, number] {
  for (const value of [width, height, aspect]) {
    if (value !== undefined && !(Number.isFinite(value) && value > 0)) {
      throw new Error('Vrm width, height, and aspect are positive numbers');
    }
  }
  const ratio = aspect ?? 16 / 9;
  if (width !== undefined && height !== undefined) return [width, height];
  if (width !== undefined) return [width, width / ratio];
  if (height !== undefined) return [height * ratio, height];
  return aspect !== undefined ? [1920, 1920 / aspect] : [1920, 1080];
}

/** The vowel weights of `mouth`, else of the lip-sync track. */
function mouthExpressions(props: VrmProps, frame: number, fps: number): Partial<Record<Vowel, number>> {
  const { mouth, lipSync } = props;
  if (typeof mouth === 'string') {
    const vowel = VOWELS[mouth];
    return vowel ? { [vowel]: 1 } : {};
  }
  if (mouth !== undefined) return { ...mouth };
  if (!lipSync) return {};
  // The shapes of this frame and the `smoothing` before it, the nearest
  // weighted most, as `<Live2D>` mixes them.
  const smoothing = Math.max(0, Math.round(props.mouthSmoothing ?? 2));
  const seconds = frame / fps;
  const sums: Partial<Record<Vowel, number>> = {};
  let total = 0;
  for (let back = 0; back <= smoothing; back += 1) {
    const weight = smoothing + 1 - back;
    const vowel = VOWELS[lipSync.mouthAtSeconds(Math.max(0, seconds - back / fps))];
    if (vowel) sums[vowel] = (sums[vowel] ?? 0) + weight;
    total += weight;
  }
  return Object.fromEntries(Object.entries(sums).map(([vowel, sum]) => [vowel, sum / total]));
}

function lights(given: VrmLights): NonNullable<VrmData['lights']> {
  const directional = given.directional ?? [];
  if (directional.length > MAX_DIRECTIONAL_LIGHTS) {
    throw new Error(`Vrm lights take at most ${MAX_DIRECTIONAL_LIGHTS} directional lights (got ${directional.length})`);
  }
  return {
    directional: directional.map((light) => ({
      direction: light.direction,
      color: linearColor(light.color ?? '#ffffff'),
      intensity: light.intensity ?? 1,
    })),
    ...(given.ambient !== undefined ? { ambient: linearColor(given.ambient) } : {}),
  };
}

/** `#rgb` or `#rrggbb` (sRGB) as linear RGB. */
function linearColor(color: string): [number, number, number] {
  const match = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color);
  if (!match) throw new Error('Vrm lights colors are #rgb or #rrggbb');
  const hex = match[1].length === 3 ? [...match[1]].map((digit) => digit + digit).join('') : match[1];
  const channel = (index: number) => {
    const value = parseInt(hex.slice(index * 2, index * 2 + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return [channel(0), channel(1), channel(2)];
}

/** The layer the plugin draws: the `.vrm` as asset `model`, and `vrmData`. */
function vrmContent(node: HostNode, walk: HostWalk): LayerContent {
  // `data` is `vrmData`'s: JSON, though its interfaces do not say so.
  const props = node.props as { src: unknown; time: number; data: JsonValue };
  return {
    type: 'plugin',
    plugin: 'vrm',
    assets: { model: walk.resolveAsset(props.src) },
    time: props.time,
    data: props.data,
  };
}

registerHostElement(ELEMENT, { content: vrmContent });
