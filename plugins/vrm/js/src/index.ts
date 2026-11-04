// The VRM plugin's components, imported by entries as `@celesta-plugin/vrm`.
// `<Vrm>` puts one layer in the scene: plugin content for plugin `vrm`, which
// the plugin's process (plugins/vrm) turns into a 3D scene of the avatar.
// The layer is as many pixels as `width` and `height` say (default
// 1920x1080); Celesta draws the scene at the layer's density, so a smaller
// layer scaled up is not sharper than this one.
//
// Like everything in a composition, the layer depends only on the frame:
// motions (`.vrma` clips) and tracks are placed on the sequence's timeline,
// blinking and lip sync come from `@celesta/character`, and every value the
// plugin gets is in `vrmData`, a pure function of the frame.

import * as React from 'react';

import { blinkPhase } from '@celesta/character';
import type { BlinkTiming, LipSyncTrack, MouthShape } from '@celesta/character';
import { useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { AssetInput, CommonProps, Easing, JsonValue, KeyframeAnimation, LayerContent } from '@celesta/react';
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

/** A VRM animation (`.vrma`), placed on the sequence's timeline. */
export interface VrmMotion {
  /** The `.vrma` (`VRMC_vrm_animation` 1.0). */
  src: AssetInput;
  /** The frame of the enclosing sequence the motion starts on. Default 0. */
  from?: number;
  /** Repeat it until the next motion starts; otherwise its last pose holds. */
  loop?: boolean;
  /** Frames over which it blends in from the motion before it. Default 0. */
  fadeIn?: number;
}

/** VRM 1.0's humanoid bones (`hips`, `head`, `leftUpperArm`, …). */
export type VrmHumanBone = (typeof HUMAN_BONES)[number];

/** A value of a track at a frame of the enclosing sequence. */
export interface VrmKeyframe {
  /** The frame; a fractional one is the nearest frame. */
  frame: number;
  value: number;
  /** How the value eases in from the previous keyframe. Default linear. */
  easing?: Easing;
}

/**
 * Keyframed turns of a bone, in degrees, added after the motions, turning
 * as `lookAt`'s angles do: `yaw` turns it to the avatar's right, `pitch`
 * lifts it to look up, `roll` tilts its top toward the avatar's right.
 * `x`, `y`, and `z` (hips only) move the avatar, in metres.
 */
export interface VrmTrack {
  pitch?: VrmKeyframe[];
  yaw?: VrmKeyframe[];
  roll?: VrmKeyframe[];
  x?: VrmKeyframe[];
  y?: VrmKeyframe[];
  z?: VrmKeyframe[];
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
  /**
   * The mouth: a vowel shape ('a' sets `aa` to 1), or the vowel weights.
   * The vowels it leaves out are 0, a motion's included; without `mouth`
   * and `lipSync`, a motion's vowels play.
   */
  mouth?: MouthShape | Partial<Record<Vowel, number>>;
  /**
   * A lip-sync track from `loadLipSync()`, used when `mouth` is not given.
   * Like `mouth`, it sets every vowel, so a motion's do not show through.
   */
  lipSync?: LipSyncTrack;
  /**
   * Frames over which a lip-sync track's shapes blend into each other: each
   * frame mixes the shapes of this many frames before it. Default 2; 0 snaps.
   */
  mouthSmoothing?: number;
  /** Blinks, seeded as `<CharacterView>` portraits do. `false` never blinks. Default `{}`. */
  blink?: BlinkTiming | false;
  /**
   * Where the eyes look: the camera, a point in the avatar's space, or
   * angles in degrees (`yaw` positive to the avatar's right, `pitch`
   * positive up).
   */
  lookAt?: 'camera' | [number, number, number] | { yaw: number; pitch: number };
  /** A framing of the humanoid, or a camera of its own. Default `'full'`. */
  camera?: 'full' | 'bust' | 'face' | VrmCamera;
  /** Default: the plugin's reference light. */
  lights?: VrmLights;
  /**
   * Motions in the order they start. Before the first the avatar rests;
   * each plays until the next starts. Expressions given here win over a
   * motion's, and `mouth` or `lipSync` sets every vowel over a motion's.
   */
  motions?: VrmMotion[];
  /** Keyframed turns of humanoid bones, and moves of the hips, on top of the motions. */
  tracks?: Partial<Record<VrmHumanBone, VrmTrack>>;
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
  motions?: VrmMotionData[];
  tracks?: Partial<Record<VrmHumanBone, Partial<Record<keyof VrmTrack, KeyframeAnimation<number>>>>>;
}

/** A motion as the plugin reads it: its clip's asset name, and seconds on the layer's clock. */
export interface VrmMotionData {
  /** `motion<i>`, `i` the index of the first motion with the same `src`. */
  clip: string;
  from: number;
  loop?: boolean;
  fadeIn?: number;
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
  'motions',
  'tracks',
] as const;

/** VRM 1.0's humanoid bones, as the plugin's `HumanBone` names them. */
const HUMAN_BONES = [
  'hips',
  'spine',
  'chest',
  'upperChest',
  'neck',
  'head',
  'leftEye',
  'rightEye',
  'jaw',
  'leftUpperLeg',
  'leftLowerLeg',
  'leftFoot',
  'leftToes',
  'rightUpperLeg',
  'rightLowerLeg',
  'rightFoot',
  'rightToes',
  'leftShoulder',
  'leftUpperArm',
  'leftLowerArm',
  'leftHand',
  'rightShoulder',
  'rightUpperArm',
  'rightLowerArm',
  'rightHand',
  'leftThumbMetacarpal',
  'leftThumbProximal',
  'leftThumbDistal',
  'leftIndexProximal',
  'leftIndexIntermediate',
  'leftIndexDistal',
  'leftMiddleProximal',
  'leftMiddleIntermediate',
  'leftMiddleDistal',
  'leftRingProximal',
  'leftRingIntermediate',
  'leftRingDistal',
  'leftLittleProximal',
  'leftLittleIntermediate',
  'leftLittleDistal',
  'rightThumbMetacarpal',
  'rightThumbProximal',
  'rightThumbDistal',
  'rightIndexProximal',
  'rightIndexIntermediate',
  'rightIndexDistal',
  'rightMiddleProximal',
  'rightMiddleIntermediate',
  'rightMiddleDistal',
  'rightRingProximal',
  'rightRingIntermediate',
  'rightRingDistal',
  'rightLittleProximal',
  'rightLittleIntermediate',
  'rightLittleDistal',
] as const;

const HUMAN_BONE_NAMES: ReadonlySet<string> = new Set(HUMAN_BONES);

const TRACK_AXES = ['pitch', 'yaw', 'roll', 'x', 'y', 'z'] as const;

export function Vrm(props: VrmProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return React.createElement(ELEMENT, vrmElementProps(props, frame, fps));
}

/**
 * The host element's props: the common ones and `src`, the time, `vrmData`,
 * and with motions their `.vrma` files by asset name (`clips`).
 */
export function vrmElementProps(props: VrmProps, frame: number, fps: number): Record<string, unknown> {
  const rest: Record<string, unknown> = { ...props };
  for (const key of DATA_PROPS) delete rest[key];
  const clips = motionClips(props.motions ?? []);
  return {
    ...rest,
    ...(Object.keys(clips).length ? { clips } : {}),
    time: frame / fps,
    data: vrmData(props, frame, fps),
  };
}

/** Each distinct `src` (`===`) once, as `motion<i>`, `i` the index of its first motion. */
function motionClips(motions: VrmMotion[]): Record<string, AssetInput> {
  const clips: Record<string, AssetInput> = {};
  motions.forEach((motion, index) => {
    if (clipIndex(motions, index) === index) clips[`motion${index}`] = motion.src;
  });
  return clips;
}

function clipIndex(motions: VrmMotion[], index: number): number {
  return motions.findIndex((motion) => motion.src === motions[index].src);
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
  const motions = motionData(props.motions ?? [], fps);
  const tracks = trackData(props.tracks ?? {}, fps);
  return {
    canvas: canvas(props),
    ...(Object.keys(expressions).length ? { expressions } : {}),
    ...(props.lookAt !== undefined ? { lookAt: props.lookAt } : {}),
    ...(props.camera !== undefined ? { camera: props.camera } : {}),
    ...(props.lights !== undefined ? { lights: lights(props.lights) } : {}),
    ...(motions.length ? { motions } : {}),
    ...(Object.keys(tracks).length ? { tracks } : {}),
  };
}

/** The motions in seconds, each naming its clip's asset. */
function motionData(motions: VrmMotion[], fps: number): VrmMotionData[] {
  return motions.map((motion, index) => {
    const from = motion.from ?? 0;
    if (!Number.isFinite(from)) throw new Error(`Vrm motion ${index} from is a finite number of frames`);
    const { fadeIn } = motion;
    if (fadeIn !== undefined && !(Number.isFinite(fadeIn) && fadeIn >= 0)) {
      throw new Error(`Vrm motion ${index} fadeIn is a number of frames, 0 or more`);
    }
    return {
      clip: `motion${clipIndex(motions, index)}`,
      from: from / fps,
      ...(motion.loop ? { loop: true } : {}),
      ...(fadeIn !== undefined ? { fadeIn: fadeIn / fps } : {}),
    };
  });
}

/**
 * The tracks as keyframe animations, checked as the plugin checks them: a
 * humanoid bone's name, the six axes, `x`, `y`, and `z` on the hips only,
 * and at least one keyframe each.
 */
function trackData(tracks: NonNullable<VrmProps['tracks']>, fps: number): NonNullable<VrmData['tracks']> {
  const data: NonNullable<VrmData['tracks']> = {};
  for (const [bone, track] of Object.entries(tracks) as [string, VrmTrack | undefined][]) {
    if (!HUMAN_BONE_NAMES.has(bone)) throw new Error(`Vrm tracks name "${bone}", which is not a VRM humanoid bone`);
    if (track === undefined) continue;
    const axes: Partial<Record<keyof VrmTrack, KeyframeAnimation<number>>> = {};
    for (const [axis, keyframes] of Object.entries(track) as [string, VrmKeyframe[] | undefined][]) {
      if (!(TRACK_AXES as readonly string[]).includes(axis)) {
        throw new Error(`Vrm tracks: ${bone} has "${axis}"; a track is pitch, yaw, roll, x, y, or z`);
      }
      if (keyframes === undefined) continue;
      if (bone !== 'hips' && (axis === 'x' || axis === 'y' || axis === 'z')) {
        throw new Error(`Vrm tracks: only hips takes x, y, and z (${bone} has ${axis})`);
      }
      if (!keyframes.length) throw new Error(`Vrm tracks: ${bone} ${axis} has no keyframes`);
      if (!keyframes.every((keyframe) => Number.isFinite(keyframe.frame) && Number.isFinite(keyframe.value))) {
        throw new Error(`Vrm tracks: ${bone} ${axis} has a keyframe whose frame or value is not a finite number`);
      }
      axes[axis as keyof VrmTrack] = keyframeAnimation(keyframes, fps);
    }
    data[bone as VrmHumanBone] = axes;
  }
  return data;
}

/**
 * `keyframes` on the layer's clock: frames of the sequence, as `<Live2D>`
 * times them at whole frame rates. A fractional frame is the nearest
 * frame at any rate, so 29.97 fps places it where 30 fps does.
 */
function keyframeAnimation(keyframes: VrmKeyframe[], fps: number): KeyframeAnimation<number> {
  // Whole frame rates time keyframes exactly; others to the microsecond.
  const timescale = Number.isInteger(fps) ? fps : 1_000_000;
  const scale = Number.isInteger(fps) ? 1 : 1_000_000 / fps;
  return {
    type: 'keyframes',
    keyframes: [...keyframes]
      .sort((a, b) => a.frame - b.frame)
      .map((keyframe) => ({
        time: { value: Math.round(Math.round(keyframe.frame) * scale), timescale },
        value: keyframe.value,
        ...(keyframe.easing ? { easing: keyframe.easing } : {}),
      })),
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

/**
 * With `mouth` or a lip-sync track, all five vowels: 0, then the chosen
 * weights. The plugin takes a motion's vowels for the keys the layer leaves
 * out, so a closed or silent mouth sends its zeros. Without either, none.
 */
function mouthExpressions(props: VrmProps, frame: number, fps: number): Partial<Record<Vowel, number>> {
  if (props.mouth === undefined && !props.lipSync) return {};
  return { aa: 0, ih: 0, ou: 0, ee: 0, oh: 0, ...mouthWeights(props, frame, fps) };
}

/** The vowel weights of `mouth`, else of the lip-sync track. */
function mouthWeights(props: VrmProps, frame: number, fps: number): Partial<Record<Vowel, number>> {
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

/**
 * The layer the plugin draws: the `.vrm` as asset `model`, each motion's
 * `.vrma` as asset `motion<i>`, and `vrmData`.
 */
function vrmContent(node: HostNode, walk: HostWalk): LayerContent {
  // `data` is `vrmData`'s: JSON, though its interfaces do not say so.
  const props = node.props as { src: unknown; clips?: Record<string, unknown>; time: number; data: JsonValue };
  const clips = Object.entries(props.clips ?? {}).map(([name, src]) => [name, walk.resolveAsset(src)]);
  return {
    type: 'plugin',
    plugin: 'vrm',
    assets: { model: walk.resolveAsset(props.src), ...Object.fromEntries(clips) },
    time: props.time,
    data: props.data,
  };
}

registerHostElement(ELEMENT, { content: vrmContent });
