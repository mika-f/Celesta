// Mounts a Celesta React composition through react-reconciler (reconciler.ts)
// and evaluates it into the same `Scene` JSON shape that
// `celesta_composition::Scene` deserializes on the Rust side. The mount is
// persistent across frames: cli.ts calls `renderAt` once per requested
// time against the same root, so component state and effects (to the extent
// a synchronous, un-scheduled reconciler runs them) carry across frames
// exactly as they would across re-renders in any other React host.

import * as React from 'react';

import { isRemoteUrl } from './entry-dir';
import { CompositionRuntimeContext, RerenderRequestContext, RootRuntimeContext, resolveTextLanguage } from './hooks';
import { TextMetricsFontsContext, withTextLanguage } from './text-measure';
import { flattenTextContent, withTextRuns } from './rich-text';
import { ProjectLayersContext, ProjectTrackLayersContext } from './hooks';
import { type HostWalk, hasHostElementPreparation, hostElement } from './host-elements';
import { resolveComponent } from './registry';
import { type HostNode, type RootContainer, HostReconciler, createRoot } from './reconciler';
import type {
  BlendMode,
  Clip,
  CompositionConfig,
  EvaluatedTransform,
  KeyframeAnimation,
  Layer,
  LayerContent,
  Paint,
  PathCommand,
  ResolvedAsset,
  Scene,
  ShaderSource,
  Stroke,
  TextStyle,
  Time,
} from './scene';
import { ShaderEffect } from './shader';
import type { JsonValue } from './generated/serde_json/JsonValue';
import { SECONDS_TIMESCALE, secondsFromTime, secondsToTime } from './time';

const HOST_TYPES = new Set([
  'composition',
  'assets',
  'asset-image',
  'asset-video',
  'asset-audio',
  'asset-font',
  'group',
  'image',
  'rect',
  'path',
  'text',
  'video',
  'audio',
  'sequence',
  'freeze-frame',
  'rawLayers',
]);
const ZERO_TIME: Time = { value: 0, timescale: 1 };

type PlaceholderConfig = Pick<CompositionConfig, 'width' | 'height' | 'durationInFrames' | 'lang'> & {
  frameRate: { numerator: number };
};
const PLACEHOLDER_CONFIG: PlaceholderConfig = {
  width: 0,
  height: 0,
  durationInFrames: 1,
  frameRate: { numerator: 1 },
};
const PLACEHOLDER_RUNTIME = {
  time: ZERO_TIME,
  width: PLACEHOLDER_CONFIG.width,
  height: PLACEHOLDER_CONFIG.height,
  fps: PLACEHOLDER_CONFIG.frameRate.numerator,
  durationInFrames: PLACEHOLDER_CONFIG.durationInFrames,
  preview: false,
};

export type EntryComponent = (props: Record<string, unknown>) => React.ReactNode;

export interface ProjectFrame {
  layers: Layer[];
  tracks: Record<string, Layer[]>;
}

/** Either a static value or the generated keyframe-animation shape — the TypeScript mirror of Rust's `Animatable<f64>`. */
type AnimatedNumber = number | KeyframeAnimation<number>;

/**
 * One audible `<Audio>` element as evaluated for a single requested frame:
 * its props plus the composition-space window any enclosing `<Sequence>`s
 * give it (`start`/`duration`), and the source position playback begins at
 * after that window clipped its head (`sourceStart`). The Rust side merges
 * these per-frame reports into the export's `AudioGraph`.
 */
export interface AudioClipDescriptor {
  src: string;
  /** Seconds into the source file the first audible moment plays. */
  sourceStart: number;
  playbackRate: AnimatedNumber;
  volume: AnimatedNumber;
  muted: boolean;
  /** Composition-space second playback starts at. */
  start: number;
  /** Audible length in composition seconds. */
  duration: number;
}

/**
 * Where the walker is inside the composition's timeline. `time` is local to
 * the enclosing `<Sequence>` chain; `originSec` converts local time back to
 * composition seconds (`local + originSec`); `rangeStartSec`/`rangeEndSec`
 * bound when the current subtree can be visible or audible, in composition
 * seconds. `compositionEndSec` is the root's `rangeEndSec`, which a
 * `<FreezeFrame>` restores for its children; `frozen` is true inside one, and
 * `idPrefix` is prepended to authored layer ids there.
 */
interface WalkContext {
  lang?: string;
  time: Time;
  fps: number;
  originSec: number;
  rangeStartSec: number;
  rangeEndSec: number;
  compositionEndSec: number;
  frozen: boolean;
  idPrefix: string;
  /** Shared by host elements for one frame; see `HostVisit.frameState`. */
  frameState: Map<unknown, unknown>;
}

export interface MountedComposition {
  /** Unmounts the persistent React root and runs effect cleanup. */
  dispose(): void;
  readonly config: CompositionConfig;
  readonly fonts: readonly ResolvedAsset[];
  /**
   * Renders one exact frame against the persistent root. Audio declarations
   * are gathered from this same tree walk — an `<Audio>` behind a
   * conditional or inside out-of-window sequences contributes on exactly the
   * frames where it actually renders.
   */
  renderAt(time: Time, project: ProjectFrame | null): { scene: Scene; audio: AudioClipDescriptor[] };
  /** Sweeps every frame without constructing visual layers; hooks and font measurements still run. */
  collectAudio(): AudioClipDescriptor[];
}

function findCompositionInstance(container: RootContainer): HostNode {
  const [instance, ...rest] = container.children;
  if (!instance || instance.type !== 'composition' || rest.length > 0) {
    throw new Error("the entry module's default export must render a single root <Composition> element");
  }
  return instance;
}

function readCompositionConfig(instance: HostNode): CompositionConfig {
  const { width, height, fps, durationInFrames, lang } = instance.props;
  if (!Number.isInteger(width) || (width as number) <= 0) {
    throw new Error('<Composition> requires a positive integer `width` prop');
  }
  if (!Number.isInteger(height) || (height as number) <= 0) {
    throw new Error('<Composition> requires a positive integer `height` prop');
  }
  if (!Number.isInteger(fps) || (fps as number) <= 0) {
    throw new Error('<Composition> requires a positive integer `fps` prop');
  }
  if (!Number.isInteger(durationInFrames) || (durationInFrames as number) <= 0) {
    throw new Error('<Composition> requires a positive integer `durationInFrames` prop');
  }
  if (lang !== undefined && typeof lang !== 'string') {
    throw new Error('<Composition> requires a string `lang` prop');
  }
  return {
    width: width as number,
    height: height as number,
    frameRate: { numerator: fps as number, denominator: 1 },
    durationInFrames: durationInFrames as number,
    ...(lang !== undefined ? { lang: lang as string } : {}),
  };
}

function rootWalkContext(fps: number, durationInFrames: number, time: Time, lang?: string): WalkContext {
  return {
    lang,
    time,
    fps,
    originSec: 0,
    rangeStartSec: 0,
    rangeEndSec: durationInFrames / fps,
    compositionEndSec: durationInFrames / fps,
    frozen: false,
    idPrefix: '',
    frameState: new Map(),
  };
}

/**
 * `value` as a flat string, so V8's JSON.stringify fast path doesn't give
 * up on the whole frame. Concatenating to 13 or more characters, or
 * slicing, makes a non-flat string; shorter strings are always flat. Ids
 * the walk builds use `join` instead, which is flat. See
 * docs/performance/react-scene-serialization.md.
 */
function flatString(value: string): string {
  return value.length < 13 ? value : JSON.parse(JSON.stringify(value));
}

function numberOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function extractTransform(props: Record<string, unknown>): EvaluatedTransform {
  // `rawTransform`/`rawOpacity` (checked here and in buildLayer) are an
  // internal escape hatch: project-runtime.ts's <ProjectTimeline /> uses
  // them to place a resolved registry component's rendered subtree at the
  // exact transform/opacity celesta-evaluator already computed for that
  // project timeline item, bypassing the flat x/y/scale/rotation props
  // authored components use. Not part of the public component prop types.
  if (props.rawTransform) {
    return props.rawTransform as EvaluatedTransform;
  }
  const scale = numberOr(props.scale, 1);
  return {
    position: { x: numberOr(props.x, 0), y: numberOr(props.y, 0) },
    scale: { x: numberOr(props.scaleX, scale), y: numberOr(props.scaleY, scale) },
    rotation: numberOr(props.rotation, 0),
    // `x`/`y` place the object's top-left corner, which reads more naturally
    // than a centre offset (this matches CSS/canvas and Remotion). The anchor
    // is also the pivot `scale`/`rotation` turn about, so pass
    // `anchorX={0.5} anchorY={0.5}` to spin/scale a component about its centre
    // — `x`/`y` then address that centre instead.
    anchor: { x: numberOr(props.anchorX, 0), y: numberOr(props.anchorY, 0) },
  };
}

/** A `<Group clip>` prop as the scene's `Clip`; `undefined` when not set. */
function extractClip(value: unknown): Clip | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  const fields = value as Record<string, unknown>;
  if (
    typeof value !== 'object' ||
    typeof fields.width !== 'number' ||
    !Number.isFinite(fields.width) ||
    typeof fields.height !== 'number' ||
    !Number.isFinite(fields.height)
  ) {
    throw new Error('<Group clip> requires finite `width` and `height`');
  }
  return {
    x: numberOr(fields.x, 0),
    y: numberOr(fields.y, 0),
    width: fields.width,
    height: fields.height,
    cornerRadius: numberOr(fields.cornerRadius, 0),
  };
}

function resolveAsset(src: unknown): ResolvedAsset {
  const reference =
    src && typeof src === 'object' && 'current' in src
      ? (src as { current?: unknown }).current
      : src;
  const path = typeof reference === 'string' ? reference : (reference as { src?: unknown } | null)?.src;
  if (typeof path !== 'string' || path.length === 0) {
    throw new Error('components with asset content require a non-empty `src` prop');
  }
  const id = typeof reference === 'string' ? reference : (reference as { id?: unknown }).id;
  const flatPath = flatString(path);
  return {
    id: typeof id === 'string' && id.length > 0 ? flatString(id) : flatPath,
    location: isRemoteUrl(flatPath) ? { type: 'url', url: flatPath } : { type: 'file', path: flatPath },
  };
}

/** A registered host element's view of the walk at `context`; see host-elements.ts. */
function hostWalk(id: string, context: WalkContext, audio: AudioClipDescriptor[]): HostWalk {
  return {
    id,
    frame: Math.round((secondsFromTime(context.time) + context.originSec) * context.fps),
    fps: context.fps,
    frozen: context.frozen,
    frameState: context.frameState,
    resolveAsset,
    layers: (node, path) => walkNode(node, path, context, audio),
    collectAudio: (props) => collectAudioClip(props, context, audio),
  };
}

function isKeyframeAnimation(value: unknown): value is KeyframeAnimation<number> {
  if (value === null || typeof value !== 'object') {
    return false;
  }
  const candidate = value as Partial<KeyframeAnimation<number>>;
  return (
    candidate.type === 'keyframes' &&
    Array.isArray(candidate.keyframes) &&
    candidate.keyframes.every(
      (keyframe) =>
        keyframe !== null &&
        typeof keyframe === 'object' &&
        typeof keyframe.value === 'number' &&
        Number.isFinite(keyframe.value) &&
        keyframe.time !== null &&
        typeof keyframe.time === 'object' &&
        Number.isFinite(secondsFromTime(keyframe.time)),
    )
  );
}

function animatedNumber(value: unknown, fallback: number): AnimatedNumber {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (isKeyframeAnimation(value)) {
    return value;
  }
  return fallback;
}

/** Leading value of an animation — used to map clip-head clipping onto the source clock. */
function firstAnimatedValue(value: AnimatedNumber): number {
  return typeof value === 'number'
    ? value
    : (value.keyframes[0]?.value ?? 0);
}

function shiftAnimated(value: AnimatedNumber, deltaSeconds: number): AnimatedNumber {
  if (typeof value === 'number' || deltaSeconds === 0) {
    return value;
  }
  return {
    ...value,
    keyframes: value.keyframes.map((keyframe) => ({
      ...keyframe,
      time: {
        value: keyframe.time.value - Math.round(deltaSeconds * keyframe.time.timescale),
        timescale: keyframe.time.timescale,
      },
    })),
  };
}

function buildLayer(
  node: HostNode,
  path: string,
  context: WalkContext,
  audio: AudioClipDescriptor[],
): Layer {
  const { props } = node;
  const id = typeof props.id === 'string' && props.id.length > 0 ? flatString(`${context.idPrefix}${props.id}`) : path;
  const transform = extractTransform(props);
  const opacity = typeof props.rawOpacity === 'number' ? props.rawOpacity : numberOr(props.opacity, 1);
  const blendMode = extractBlendMode(props.blendMode);
  const effects = extractEffects(props, context);

  let content: LayerContent;
  if (node.type === 'group' || node.type === 'sequence') {
    const clip = node.type === 'group' ? extractClip(props.clip) : undefined;
    content = {
      type: 'group',
      layers: walkChildren(node, path, context, audio),
      ...(clip ? { clip } : {}),
    };
  } else if (node.type === 'freeze-frame') {
    // Frozen children are always silent: their audio goes nowhere.
    content = { type: 'group', layers: walkChildren(node, path, childFreezeContext(node, context, id), []) };
  } else if (node.type === 'image') {
    for (const key of ['width', 'height']) {
      if (props[key] !== undefined && (typeof props[key] !== 'number' || !Number.isFinite(props[key]) || props[key] <= 0)) {
        throw new Error(`Image ${key} must be finite and positive`);
      }
    }
    if (props.fit !== undefined && props.fit !== 'contain' && props.fit !== 'cover') {
      throw new Error('Image fit must be contain or cover');
    }
    content = { type: 'image', asset: resolveAsset(props.src),
      ...(props.width === undefined ? {} : { width: props.width as number }),
      ...(props.height === undefined ? {} : { height: props.height as number }),
      ...(props.fit === undefined ? {} : { fit: props.fit as 'contain' | 'cover' }),
    };
  } else if (hostElement(node.type)?.content) {
    content = hostElement(node.type)!.content!(node, hostWalk(id, context, audio));
  } else if (node.type === 'text') {
    const maxWidth = props.maxWidth;
    const flat = flattenTextContent(props.children as React.ReactNode, '<Text> children');
    content = {
      type: 'text',
      text: flatString(flat.text),
      style: withTextRuns(
        withTextLanguage(
          (props.style as TextStyle | undefined) ?? {},
          resolveTextLanguage(props.lang, context.lang),
        ),
        flat,
        '<Text>',
      ),
      ...(typeof maxWidth === 'number' ? { maxWidth } : {}),
      ...(props.anchorY === 'baseline' ? { baselineAnchor: true } : {}),
    };
  } else if (node.type === 'rect') {
    const fill = toPaint(props.fill);
    const strokePaint = toPaint(props.stroke);
    const strokeWidth = numberOr(props.strokeWidth, 0);
    const stroke: Stroke | undefined =
      strokePaint && strokeWidth > 0 ? { paint: strokePaint, width: strokeWidth } : undefined;
    content = {
      type: 'rect',
      width: numberOr(props.width, 0),
      height: numberOr(props.height, 0),
      ...(fill ? { fill } : {}),
      ...(stroke ? { stroke } : {}),
      cornerRadius: numberOr(props.cornerRadius, 0),
    };
  } else if (node.type === 'path') {
    const fill = toPaint(props.fill);
    const strokePaint = toPaint(props.stroke);
    const strokeWidth = numberOr(props.strokeWidth, 0);
    const miterLimit = props.miterLimit;
    if (miterLimit !== undefined && (typeof miterLimit !== 'number' || !Number.isFinite(miterLimit) || miterLimit < 1)) {
      throw new Error('<Path> miterLimit must be a finite number of at least 1');
    }
    content = {
      type: 'path',
      commands: extractPathCommands(props.commands),
      ...(fill ? { fill } : {}),
      ...(strokePaint && strokeWidth > 0 ? { stroke: { paint: strokePaint, width: strokeWidth } } : {}),
      ...(props.cap && props.cap !== 'butt' ? { lineCap: extractOneOf(props.cap, LINE_CAPS, 'cap') } : {}),
      ...(props.join && props.join !== 'miter' ? { lineJoin: extractOneOf(props.join, LINE_JOINS, 'join') } : {}),
      ...(miterLimit !== undefined && miterLimit !== 4 ? { miterLimit } : {}),
    };
  } else if (node.type === 'video') {
    // A React <Video> plays synced to the enclosing sequence chain's own
    // clock from its frame 0: `context.time` here *is* the video's local
    // time. `sourceTimeSeconds` — the only field GpuRenderer actually reads
    // to seek/decode — is `startFrom` plus local time scaled by
    // `playbackRate`, mirroring how celesta-evaluator derives it for a
    // project TimelineContent::Video at a constant playback rate.
    const startFrom = numberOr(props.startFrom, 0);
    const playbackRate = numberOr(props.playbackRate, 1);
    content = {
      type: 'video',
      asset: resolveAsset(props.src),
      timing: {
        localTime: context.time,
        sourceStart: secondsToTime(startFrom),
        sourceTimeSeconds: startFrom + secondsFromTime(context.time) * playbackRate,
        playbackRate,
      },
    };
  } else {
    throw new Error(`unreachable: unknown host node type "${node.type}"`);
  }

  return { id, transform, opacity, ...(blendMode !== 'normal' ? { blendMode } : {}),
    ...(effects ? { effects } : {}), content };
}

function toPaint(value: unknown): Paint | undefined {
  return typeof value === 'string'
    ? { type: 'solid', color: flatString(value) }
    : value && typeof value === 'object'
      ? (value as Paint)
      : undefined;
}

const LINE_CAPS = ['butt', 'round', 'square'] as const;
const LINE_JOINS = ['miter', 'round', 'bevel'] as const;

function extractOneOf<T extends string>(value: unknown, allowed: readonly T[], name: string): T {
  if (!allowed.includes(value as T)) {
    throw new Error(`<Path> ${name} must be one of ${allowed.map((v) => `'${v}'`).join(', ')}`);
  }
  return value as T;
}

/** The finite coordinates each path command type carries. */
const PATH_COMMAND_FIELDS: Record<PathCommand['type'], readonly string[]> = {
  moveTo: ['x', 'y'],
  lineTo: ['x', 'y'],
  quadTo: ['x1', 'y1', 'x', 'y'],
  cubicTo: ['x1', 'y1', 'x2', 'y2', 'x', 'y'],
  close: [],
};

function extractPathCommands(value: unknown): PathCommand[] {
  if (!Array.isArray(value)) {
    throw new Error('<Path> requires a `commands` array');
  }
  for (const command of value as Array<Record<string, unknown> | null>) {
    const type = command?.type;
    const fields = typeof type === 'string' && Object.prototype.hasOwnProperty.call(PATH_COMMAND_FIELDS, type)
      ? PATH_COMMAND_FIELDS[type as PathCommand['type']]
      : undefined;
    if (!fields) {
      throw new Error(`<Path> command type must be moveTo, lineTo, quadTo, cubicTo, or close, not ${JSON.stringify(type)}`);
    }
    for (const field of fields) {
      const coordinate = command![field];
      if (typeof coordinate !== 'number' || !Number.isFinite(coordinate)) {
        throw new Error(`<Path> ${type} command requires a finite \`${field}\``);
      }
    }
  }
  return value as PathCommand[];
}

/** The frame's custom shaders, each once, in `WalkContext.frameState`. */
const SHADER_SOURCES = Symbol('shader sources');

/** The custom shaders the frame walked with `context` used, each once. */
function usedShaders(context: WalkContext): ShaderSource[] {
  return (context.frameState.get(SHADER_SOURCES) as ShaderSource[] | undefined) ?? [];
}

function sameShaderSource(a: ShaderSource, b: ShaderSource): boolean {
  return a.wgsl === b.wgsl && JSON.stringify(a.params ?? []) === JSON.stringify(b.params ?? []);
}

function extractShader(value: unknown, context: WalkContext): NonNullable<Layer['effects']>['shader'] {
  if (value === undefined) return undefined;
  if (!(value instanceof ShaderEffect)) {
    throw new Error("shader must be a value returned by a shader from @celesta/shader's defineShader()");
  }
  const sources = usedShaders(context);
  context.frameState.set(SHADER_SOURCES, sources);
  const { source } = ShaderEffect.source(value);
  // A frame uses a few shaders. Keying a Map by the id would internalize
  // it into a thin string, which `flat-strings.test.mjs` rejects.
  const used = sources.find((other) => other.id === source.id);
  if (!used) {
    sources.push(source);
  } else if (used !== source && !sameShaderSource(used, source)) {
    // The scene names each source once, so two shaders whose ids collide
    // cannot both reach the renderer.
    throw new Error(`two different shaders share the id ${source.id}; change either one's source`);
  }
  return ShaderEffect.serialize(value);
}

function extractEffects(props: Record<string, unknown>, context: WalkContext): Layer['effects'] | undefined {
  const radius = (value: unknown, name: string): number => {
    if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > 64) {
      throw new Error(`${name} must be a finite number between 0 and 64`);
    }
    return value;
  };
  const offset = (value: unknown, name: string): number => {
    if (typeof value !== 'number' || !Number.isFinite(value)) {
      throw new Error(`${name} must be a finite number`);
    }
    return value;
  };
  const color = (value: unknown, name: string): string => {
    if (typeof value !== 'string' || !/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value)) {
      throw new Error(`${name} must be a #RRGGBB or #RRGGBBAA color`);
    }
    return value;
  };
  const blur = props.blur === undefined ? 0 : radius(props.blur, 'blur');
  if (props.shadow === null || props.glow === null) {
    throw new Error('shadow and glow must be effect objects when provided');
  }
  const shadow = props.shadow === undefined ? undefined : props.shadow as Record<string, unknown>;
  const glow = props.glow === undefined ? undefined : props.glow as Record<string, unknown>;
  const shader = extractShader(props.shader, context);
  if (blur === 0 && !shadow && !glow && !shader) return undefined;
  return {
    blur,
    ...(shader ? { shader } : {}),
    ...(shadow ? { shadow: {
      color: color(shadow.color, 'shadow.color'),
      blur: radius(shadow.blur, 'shadow.blur'),
      offsetX: offset(shadow.offsetX, 'shadow.offsetX'),
      offsetY: offset(shadow.offsetY, 'shadow.offsetY'),
    } } : {}),
    ...(glow ? { glow: {
      color: color(glow.color, 'glow.color'),
      blur: radius(glow.blur, 'glow.blur'),
    } } : {}),
  };
}

const BLEND_MODES: readonly BlendMode[] = ['normal', 'multiply', 'screen', 'overlay', 'add', 'difference'];

function extractBlendMode(value: unknown): BlendMode {
  if (value === undefined) {
    return 'normal';
  }
  if (!BLEND_MODES.includes(value as BlendMode)) {
    throw new Error(`unknown blendMode ${JSON.stringify(value)}; expected one of ${BLEND_MODES.join(', ')}`);
  }
  return value as BlendMode;
}

/**
 * The `<Sequence>`-shifted walk context for this node's children, or null
 * when the sequence's window does not contain the currently rendered local
 * time (its whole subtree then contributes neither layers nor audio).
 */
function childSequenceContext(node: HostNode, context: WalkContext): WalkContext | null {
  const { props } = node;
  const fps = context.fps;
  const fromFrames = numberOr(props.from, 0);
  const startLocalSec = fromFrames / fps;
  const endLocalSec =
    typeof props.durationInFrames === 'number' && Number.isFinite(props.durationInFrames)
      ? (fromFrames + Math.max(0, props.durationInFrames)) / fps
      : context.rangeEndSec - context.originSec;
  const localSec = secondsFromTime(context.time);
  if (localSec < startLocalSec || localSec >= endLocalSec) {
    return null;
  }
  const originSec = context.originSec + startLocalSec;
  const rangeStartSec = Math.max(context.rangeStartSec, originSec);
  const rangeEndSec = Math.min(context.rangeEndSec, context.originSec + endLocalSec);
  if (rangeEndSec <= rangeStartSec) {
    return null;
  }
  return {
    time: secondsToTime(localSec - startLocalSec),
    lang: context.lang,
    fps,
    originSec,
    rangeStartSec,
    rangeEndSec,
    compositionEndSec: context.compositionEndSec,
    frozen: context.frozen,
    idPrefix: context.idPrefix,
    frameState: context.frameState,
  };
}

/**
 * The walk context for a `<FreezeFrame>`'s children: the root clock at its
 * `frame`, with the root's range rather than the enclosing one, so a freeze
 * inside a short `<Sequence>` still opens the sequences its frame is in.
 * Authored ids inside are prefixed with the freeze layer's id.
 */
function childFreezeContext(node: HostNode, context: WalkContext, layerId: string): WalkContext {
  return {
    time: secondsToTime(numberOr(node.props.frame, 0) / context.fps),
    lang: context.lang,
    fps: context.fps,
    originSec: 0,
    rangeStartSec: 0,
    rangeEndSec: context.compositionEndSec,
    compositionEndSec: context.compositionEndSec,
    frozen: true,
    idPrefix: `${layerId}/`,
    frameState: context.frameState,
  };
}

/** Runs registered host elements' `prepare` over the nodes this frame shows. */
function prepareHostElements(node: HostNode, context: WalkContext): void {
  const childContext =
    node.type === 'sequence'
      ? childSequenceContext(node, context)
      : node.type === 'freeze-frame'
        ? childFreezeContext(node, context, '')
        : context;
  if (!childContext) {
    return;
  }
  hostElement(node.type)?.prepare?.(node, context);
  for (const child of node.children) {
    prepareHostElements(child, childContext);
  }
}

function collectAudioClip(props: Record<string, unknown>, context: WalkContext, results: AudioClipDescriptor[]): void {
  const source = resolveAsset(props.src);
  // An audio clip can play no earlier than its own local zero and not at
  // all outside the enclosing sequence window.
  const startSec = Math.max(context.rangeStartSec, context.originSec);
  const endSec = context.rangeEndSec;
  if (endSec <= startSec) {
    return;
  }
  const startFrom = numberOr(props.startFrom, 0);
  const playbackRate = animatedNumber(props.playbackRate, 1);
  const volume = animatedNumber(props.volume, 1);
  // When the window clips the clip's head (an outer sequence started before
  // this clip's local zero, or negative `from`s stacked up), both the
  // source position and any animation keyframes shift by the clipped
  // amount so the curve stays aligned with what is actually heard.
  const clipped = startSec - context.originSec;
  results.push({
    src: source.location.type === 'file' ? source.location.path : source.location.url,
    sourceStart: startFrom + clipped * firstAnimatedValue(playbackRate),
    playbackRate: shiftAnimated(playbackRate, -clipped),
    volume: shiftAnimated(volume, -clipped),
    muted: props.muted === true,
    start: startSec,
    duration: endSec - startSec,
  });
}

function walkNode(
  node: HostNode,
  path: string,
  context: WalkContext,
  audio: AudioClipDescriptor[],
  audioOnly = false,
): Layer[] {
  const element = hostElement(node.type);
  if (element) {
    element.audio?.(node, hostWalk(path, context, audio));
    return audioOnly || !element.content ? [] : [buildLayer(node, path, context, audio)];
  }
  if (!HOST_TYPES.has(node.type)) {
    throw new Error(`unsupported element <${node.type}>; use Celesta's built-in components`);
  }
  if ((node.type === 'group' || node.type === 'sequence') && node.props.lang !== undefined) {
    context = { ...context, lang: resolveTextLanguage(node.props.lang, context.lang) };
  }
  if (node.type === 'rawLayers') {
    return audioOnly ? [] : inheritTextLanguage((node.props.layers as Layer[] | undefined) ?? [], context.lang);
  }
  if (
    node.type === 'assets' ||
    node.type === 'asset-image' ||
    node.type === 'asset-video' ||
    node.type === 'asset-audio' ||
    node.type === 'asset-font'
  ) {
    return [];
  }
  if (node.type === 'audio') {
    // <Audio> contributes no visual Layer (there is no LayerContent audio
    // variant — audio is a separate top-level AudioGraph); it is collected
    // during this same walk instead.
    collectAudioClip(node.props, context, audio);
    return [];
  }
  if (node.type === 'sequence') {
    const childContext = childSequenceContext(node, context);
    if (!childContext) {
      return [];
    }
    return audioOnly
      ? walkChildren(node, path, childContext, audio, true)
      : [buildLayer(node, path, childContext, audio)];
  }
  if (audioOnly) {
    // A <FreezeFrame> subtree is silent, so the audio sweep skips it whole.
    return node.type === 'group' ? walkChildren(node, path, context, audio, true) : [];
  }
  return [buildLayer(node, path, context, audio)];
}

/**
 * Gathers every `<Font>` in the tree, wherever it is declared (inside
 * `<Assets>` or anywhere else), into the scene's font list so text layout
 * can use it. A relative `src` resolves against the entry's directory, like
 * every other asset. Declarations repeating the same id and path are merged.
 */
function collectFonts(node: HostNode, fonts: ResolvedAsset[]): void {
  if (node.type === 'asset-font') {
    const asset = resolveAsset(node.props.src);
    const name = node.props.name;
    const font = typeof name === 'string' && name.length > 0 ? { ...asset, id: name } : asset;
    if (!fonts.some((existing) => existing.id === font.id && sameLocation(existing, font))) {
      fonts.push(font);
    }
  }
  for (const child of node.children) {
    collectFonts(child, fonts);
  }
}

function sameLocation(left: ResolvedAsset, right: ResolvedAsset): boolean {
  return JSON.stringify(left.location) === JSON.stringify(right.location);
}

function walkChildren(
  node: HostNode,
  parentPath: string,
  context: WalkContext,
  audio: AudioClipDescriptor[],
  audioOnly = false,
): Layer[] {
  if (audioOnly) {
    // Audio has no layer ids: avoid allocating paths and flattening empty
    // layer arrays for every visual node in a dense composition.
    for (const child of node.children) walkNode(child, '', context, audio, true);
    return [];
  }
  return node.children.flatMap((child, index) =>
    walkNode(child, [parentPath, index].join('.'), context, audio),
  );
}

/** Includes text supplied by a project timeline and its nested groups. */
function inheritTextLanguage(layers: Layer[], lang?: string): Layer[] {
  if (lang === undefined) return layers;
  return layers.map((layer) => {
    const content = layer.content;
    if (content.type === 'text') {
      return { ...layer, content: { ...content, style: withTextLanguage(content.style, lang) } };
    }
    if (content.type === 'group') {
      return { ...layer, content: { ...content, layers: inheritTextLanguage(content.layers, lang) } };
    }
    return layer;
  });
}

export function mount(defaultExport: EntryComponent): MountedComposition {
  const { container, root } = createRoot();
  let fonts: ResolvedAsset[] = [];
  let rerenderRequested = false;
  const requestRerender = () => {
    rerenderRequested = true;
  };

  const renderTree = (
    time: Time,
    project: ProjectFrame | null,
    config: CompositionConfig | PlaceholderConfig,
  ) => {
    const runtimeValue = {
      time,
      width: config.width,
      height: config.height,
      fps: config.frameRate.numerator,
      durationInFrames: config.durationInFrames,
      preview: false,
      lang: config.lang,
    };
    const element = React.createElement(
      ProjectLayersContext.Provider,
      { value: project?.layers ?? null },
      React.createElement(
        ProjectTrackLayersContext.Provider,
        { value: project?.tracks ?? null },
        React.createElement(
          RootRuntimeContext.Provider,
          { value: runtimeValue },
          React.createElement(
            CompositionRuntimeContext.Provider,
            { value: runtimeValue },
            React.createElement(
              TextMetricsFontsContext.Provider,
              { value: fonts },
              React.createElement(
                RerenderRequestContext.Provider,
                { value: requestRerender },
                React.createElement(defaultExport, {}),
              ),
            ),
          ),
        ),
      ),
    );
    HostReconciler.flushSync(() => {
      HostReconciler.updateContainer(element, root, null, null);
    });
  };

  // The first pass reads <Composition>'s size and clock props, which must
  // be static (not derived from useVideoConfig()/useCurrentFrame()/
  // <ProjectTimeline />/<ProjectTrack />). lang may vary by frame. Children still render
  // and may use those, so a placeholder context is provided rather than
  // leaving it unset, which would throw. Empty layers/tracks (rather than
  // null, which would still throw) are enough since this pass's own output
  // layers are discarded.
  renderTree(ZERO_TIME, { layers: [], tracks: {} }, PLACEHOLDER_CONFIG);
  const compositionInstance = findCompositionInstance(container);
  const config = readCompositionConfig(compositionInstance);
  const initialFonts: ResolvedAsset[] = [];
  collectFonts(compositionInstance, initialFonts);
  fonts = initialFonts;

  const renderFrame = (time: Time, project: ProjectFrame | null, audioOnly = false) => {
    rerenderRequested = false;
    renderTree(time, project, config);
    let instance = findCompositionInstance(container);
    const nextFonts: ResolvedAsset[] = [];
    collectFonts(instance, nextFonts);
    const fontsChanged = JSON.stringify(nextFonts) !== JSON.stringify(fonts);
    if (fontsChanged || rerenderRequested) {
      fonts = nextFonts;
      // A declaration can appear after its consumer, or change this frame,
      // and a ref read during render is attached only by the commit.
      // Reconcile again with the complete font list and refs before
      // emitting layers.
      rerenderRequested = false;
      renderTree(time, project, config);
      instance = findCompositionInstance(container);
    }
    const audio: AudioClipDescriptor[] = [];
    const context = rootWalkContext(
      config.frameRate.numerator,
      config.durationInFrames,
      time,
      resolveTextLanguage(instance.props.lang),
    );
    if (!audioOnly && hasHostElementPreparation()) {
      for (const child of instance.children) {
        prepareHostElements(child, context);
      }
    }
    const layers = walkChildren(
      instance,
      'root',
      context,
      audio,
      audioOnly,
    );
    const shaders = usedShaders(context);
    return {
      scene: {
        width: config.width,
        height: config.height,
        frameRate: config.frameRate,
        time,
        ...(fonts.length > 0 ? { fonts } : {}),
        ...(shaders.length > 0 ? { shaders } : {}),
        layers,
      },
      audio,
    };
  };

  return {
    dispose() { HostReconciler.flushSync(() => { HostReconciler.updateContainer(null, root, null, null); }); },
    config,
    get fonts() { return fonts; },
    renderAt: renderFrame,
    collectAudio() {
      const audio: AudioClipDescriptor[] = [];
      for (let frame = 0; frame < config.durationInFrames; frame++) {
        const time = { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator };
        for (const clip of renderFrame(time, null, true).audio) audio.push(clip);
      }
      return audio;
    },
  };
}

export interface ComponentResolutionRequest {
  component: string;
  props: Record<string, JsonValue>;
}

/**
 * The composition facts a resolved component's hooks should see: everything
 * `useVideoConfig()`/`useCurrentFrame()` read, matching the entry's own
 * `<Composition>` (the Rust bridge derives it from the same handshake
 * metadata the export path renders against), plus the exact frame time the
 * resolution was requested for. Omitted by older bridges — components then
 * see the placeholder runtime (frame 0) they always saw.
 */
export interface ResolutionRuntime {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  time: Time;
  /** Absent from older bridges, which means export/non-preview mode. */
  preview?: boolean;
}

/** One resolution outcome: the component's layers, or null when its name has no registerComponent() match. */
export type ComponentResolution = Layer[] | null;

export interface Resolver {
  /**
   * Renders each request's registered component against a dedicated
   * persistent root (separate from the composition's, so hook state in
   * resolved components survives across calls exactly as it does for the
   * main tree), returning the layers each one produced — or null for a
   * name with no matching registerComponent() call. `runtime`, when given,
   * is what those components' `useCurrentFrame()`/`useVideoConfig()` see;
   * without one they fall back to the placeholder frame-0 runtime.
   */
  resolve(
    items: readonly ComponentResolutionRequest[],
    runtime?: ResolutionRuntime,
    fonts?: readonly ResolvedAsset[],
  ): (Layer[] | null)[];
}

/**
 * Standalone component resolver used by the editor preview: resolves
 * individual `registerComponent()` names without going through a
 * `<Composition>` tree. `lang` supplies that composition's text language.
 */
export function createResolver(lang?: string): Resolver {
  const { container, root } = createRoot();

  const ResolverHost = ({ items }: { items: readonly ComponentResolutionRequest[] }) =>
    React.createElement(
      React.Fragment,
      null,
      items.map((item, index) => {
        const definition = resolveComponent(item.component);
        if (!definition) {
          return React.createElement('rawLayers', { key: index, layers: [] });
        }
        return React.createElement(
          'group',
          { key: index, id: `resolve.${index}` },
          React.createElement(definition, item.props),
        );
      }),
    );

  return {
    resolve(items, runtime, fonts = []) {
      const runtimeValue = {
        ...(runtime ?? PLACEHOLDER_RUNTIME),
        preview: runtime?.preview === true,
        lang,
      };
      let rerenderRequested = false;
      const requestRerender = () => {
        rerenderRequested = true;
      };
      const element = () => React.createElement(
        ProjectLayersContext.Provider,
        { value: [] },
        React.createElement(
          ProjectTrackLayersContext.Provider,
          { value: {} },
          React.createElement(
            RootRuntimeContext.Provider,
            { value: runtimeValue },
            React.createElement(
              CompositionRuntimeContext.Provider,
              { value: runtimeValue },
              React.createElement(
                TextMetricsFontsContext.Provider,
                { value: fonts },
                React.createElement(
                  RerenderRequestContext.Provider,
                  { value: requestRerender },
                  React.createElement(ResolverHost, { items }),
                ),
              ),
            ),
          ),
        ),
      );
      HostReconciler.flushSync(() => {
        HostReconciler.updateContainer(element(), root, null, null);
      });
      if (rerenderRequested) {
        // A ref read during render is attached only by the commit.
        HostReconciler.flushSync(() => {
          HostReconciler.updateContainer(element(), root, null, null);
        });
      }
      const walkContext = rootWalkContext(
        runtimeValue.fps,
        runtimeValue.durationInFrames,
        runtime?.time ?? ZERO_TIME,
        runtimeValue.lang,
      );
      return container.children.map((child, index) => {
        // An unresolved name renders the empty 'rawLayers' marker; anything
        // else is a resolved component's group (possibly one that rendered
        // no layers, which is still "resolved").
        if (child.type === 'rawLayers') {
          return null;
        }
        const audio: AudioClipDescriptor[] = [];
        const layers = walkChildren(child, `resolve.${index}`, walkContext, audio);
        // Resolutions carry only layers, so a shader's source has no way
        // into the project's scene.
        if (usedShaders(walkContext).length > 0) {
          throw new Error(`${items[index]!.component}: custom shaders are not supported in components placed on project timelines`);
        }
        return layers;
      });
    },
  };
}
