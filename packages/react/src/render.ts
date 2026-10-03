// Mounts a Celesta React composition through react-reconciler (reconciler.ts)
// and evaluates it into the same `Scene` JSON shape that
// `celesta_composition::Scene` deserializes on the Rust side. The mount is
// persistent across frames: cli.ts calls `renderAt` once per requested
// time against the same root, so component state and effects (to the extent
// a synchronous, un-scheduled reconciler runs them) carry across frames
// exactly as they would across re-renders in any other React host.

import * as React from 'react';

import { isRemoteUrl } from './entry-dir';
import { CompositionRuntimeContext } from './hooks';
import { TextMetricsFontsContext } from './text-measure';
import { ProjectLayersContext, ProjectTrackLayersContext } from './project-runtime';
import { resolveVisibleLayers } from './psd-preset';
import type { PsdCharacterBlink, PsdCharacterLipSync, PsdExpression } from './components';
import { type BlinkPhase, type BlinkTiming, blinkPhase } from './blink';
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
  Stroke,
  TextStyle,
  Time,
} from './scene';
import type { JsonValue } from './generated/serde_json/JsonValue';
import { SECONDS_TIMESCALE, secondsFromTime, secondsToTime } from './time';

const HOST_TYPES = new Set([
  'composition',
  'assets',
  'asset-character',
  'asset-image',
  'asset-video',
  'asset-audio',
  'asset-font',
  'character-view',
  'dialogue',
  'group',
  'image',
  'rect',
  'path',
  'text',
  'video',
  'audio',
  'sequence',
  'rawLayers',
]);
const ZERO_TIME: Time = { value: 0, timescale: 1 };

type PlaceholderConfig = Pick<CompositionConfig, 'width' | 'height' | 'durationInFrames'> & {
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
 * seconds.
 */
interface WalkContext {
  time: Time;
  fps: number;
  originSec: number;
  rangeStartSec: number;
  rangeEndSec: number;
  characterViewOverrides: Map<HostNode, Record<string, unknown>>;
}

export interface MountedComposition {
  readonly config: CompositionConfig;
  readonly fonts: readonly ResolvedAsset[];
  /**
   * Renders one exact frame against the persistent root. Audio declarations
   * are gathered from this same tree walk — an `<Audio>` behind a
   * conditional or inside out-of-window sequences contributes on exactly the
   * frames where it actually renders.
   */
  renderAt(time: Time, project: ProjectFrame | null): { scene: Scene; audio: AudioClipDescriptor[] };
}

function findCompositionInstance(container: RootContainer): HostNode {
  const [instance, ...rest] = container.children;
  if (!instance || instance.type !== 'composition' || rest.length > 0) {
    throw new Error("the entry module's default export must render a single root <Composition> element");
  }
  return instance;
}

function readCompositionConfig(instance: HostNode): CompositionConfig {
  const { width, height, fps, durationInFrames } = instance.props;
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
  return {
    width: width as number,
    height: height as number,
    frameRate: { numerator: fps as number, denominator: 1 },
    durationInFrames: durationInFrames as number,
  };
}

function rootWalkContext(fps: number, durationInFrames: number, time: Time): WalkContext {
  return {
    time,
    fps,
    originSec: 0,
    rangeStartSec: 0,
    rangeEndSec: durationInFrames / fps,
    characterViewOverrides: new Map(),
  };
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

function extractText(children: unknown): string {
  if (typeof children === 'string') {
    return children;
  }
  if (typeof children === 'number') {
    return String(children);
  }
  if (Array.isArray(children)) {
    return children.map(extractText).join('');
  }
  throw new Error('<Text> children must be a string, a number, or an array of those');
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
  return {
    id: typeof id === 'string' && id.length > 0 ? id : path,
    location: isRemoteUrl(path) ? { type: 'url', url: path } : { type: 'file', path },
  };
}

function resolveReference(value: unknown): unknown {
  return value && typeof value === 'object' && 'current' in value
    ? (value as { current?: unknown }).current
    : value;
}

/** The timing fields of `value` that are set, so spreading it never clears a fallback. */
function definedTiming(value: BlinkTiming): BlinkTiming {
  const timing: BlinkTiming = {};
  if (value.interval !== undefined) timing.interval = value.interval;
  if (value.duration !== undefined) timing.duration = value.duration;
  if (value.seed !== undefined) timing.seed = value.seed;
  return timing;
}

function resolveCharacterView(value: unknown): HostNode | null {
  const node = resolveReference(value);
  return node !== null && typeof node === 'object' && (node as HostNode).type === 'character-view'
    ? (node as HostNode)
    : null;
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
  const id = typeof props.id === 'string' && props.id.length > 0 ? props.id : path;
  const transform = extractTransform(props);
  const opacity = typeof props.rawOpacity === 'number' ? props.rawOpacity : numberOr(props.opacity, 1);
  const blendMode = extractBlendMode(props.blendMode);
  const effects = extractEffects(props);

  let content: LayerContent;
  if (node.type === 'group' || node.type === 'sequence') {
    const clip = node.type === 'group' ? extractClip(props.clip) : undefined;
    content = {
      type: 'group',
      layers: walkChildren(node, path, context, audio),
      ...(clip ? { clip } : {}),
    };
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
  } else if (node.type === 'character-view') {
    const override = context.characterViewOverrides.get(node);
    const character = resolveReference(props.character) as
      | {
          id?: string;
          portrait?:
            | {
                type?: 'image';
                defaultExpression: string;
                expressions: Record<string, unknown>;
                lipSync?: {
                  a: unknown;
                  i: unknown;
                  u: unknown;
                  e: unknown;
                  o: unknown;
                  closed?: unknown;
                };
                blink?: BlinkTiming & {
                  closed: Record<string, unknown>;
                  half?: Record<string, unknown>;
                  overlay?: boolean;
                };
              }
            | {
                type: 'psd';
                src: unknown;
                layers?: string[] | string;
                expressions?: Record<string, PsdExpression>;
                defaultExpression?: string;
                lipSync?: PsdCharacterLipSync;
                blink?: PsdCharacterBlink;
              };
        }
      | null;
    const portrait = character?.portrait;
    if (!portrait) {
      throw new Error('<CharacterView> requires a character with a portrait');
    }
    const mouthValue = override?.mouth ?? props.mouth;
    const mouth = typeof mouthValue === 'string' ? mouthValue : undefined;
    // Blinks follow the composition frame, not the local one, so a cut to a
    // new <Sequence> does not restart (and re-sync) every character's rhythm.
    const eyes = (timing: BlinkTiming): BlinkPhase => {
      if (props.blink === false) {
        return 'open';
      }
      const frame = Math.round((secondsFromTime(context.time) + context.originSec) * context.fps);
      const viewTiming = typeof props.blink === 'object' && props.blink !== null ? (props.blink as BlinkTiming) : {};
      return blinkPhase(frame, context.fps, {
        seed: character?.id,
        ...definedTiming(timing),
        ...definedTiming(viewTiming),
      });
    };
    if (portrait.type === 'psd') {
      const expressionValue = override?.expression ?? props.expression;
      const expressionName = typeof expressionValue === 'string' ? expressionValue : portrait.defaultExpression;
      let expression: PsdExpression | undefined;
      if (expressionName !== undefined) {
        expression = portrait.expressions?.[expressionName];
        if (expression === undefined) {
          throw new Error(`character has no expression "${expressionName}"`);
        }
      }
      // An expression is its layers, or `{ layers, lipSync, blink }` with mouths or eyes of its own.
      const { layers: expressionLayers, lipSync: expressionLipSync, blink: expressionBlink } =
        typeof expression === 'object' && !Array.isArray(expression)
          ? expression
          : { layers: expression, lipSync: undefined, blink: undefined };
      const lipSync = expressionLipSync ?? portrait.lipSync;
      // An expression's eyes replace the portrait's as a set, so a half-shut
      // layer from another face folder never leaks in; timing still falls back.
      const blink: PsdCharacterBlink | undefined =
        expressionBlink === false
          ? undefined
          : expressionBlink
            ? { ...definedTiming(portrait.blink ?? {}), ...expressionBlink }
            : portrait.blink;
      const selectedLayer = mouth && lipSync
        ? mouth === 'closed'
          ? lipSync.closed
          : lipSync[mouth as 'a' | 'i' | 'u' | 'e' | 'o']
        : undefined;
      const mouthLayers = lipSync ? Object.values(lipSync) : [];
      const layerList = (layers: string[] | string | undefined) =>
        Array.isArray(layers) ? layers : typeof layers === 'string' ? resolveVisibleLayers(layers) : [];
      const visibleLayers = [...new Set([...layerList(portrait.layers), ...layerList(expressionLayers)])];
      const enabledLayers = selectedLayer ? [selectedLayer] : [];
      const disabledLayers = selectedLayer ? mouthLayers.filter((layer) => layer !== selectedLayer) : [];
      if (blink) {
        // Eye paths are layer paths, never layer-state strings: a bare string is one path.
        const eyeLayers = (layers: string[] | string | undefined) =>
          layers === undefined ? [] : Array.isArray(layers) ? layers : [layers];
        const open = eyeLayers(blink.open);
        const closed = eyeLayers(blink.closed);
        const half = eyeLayers(blink.half);
        let phase = eyes(blink);
        if (phase === 'half' && half.length === 0) {
          phase = 'open';
        }
        const shown = phase === 'closed' ? closed : phase === 'half' ? half : open;
        enabledLayers.push(...shown);
        disabledLayers.push(...[...open, ...closed, ...half].filter((layer) => !shown.includes(layer)));
      }
      content = {
        type: 'psd',
        asset: resolveAsset(portrait.src),
        ...(visibleLayers.length ? { visibleLayers } : {}),
        ...(enabledLayers.length ? { enabledLayers: [...new Set(enabledLayers)] } : {}),
        ...(disabledLayers.length ? { disabledLayers: [...new Set(disabledLayers)] } : {}),
      };
    } else {
      const expressionValue = override?.expression ?? props.expression;
      const expression = typeof expressionValue === 'string' ? expressionValue : portrait.defaultExpression;
      const src = portrait.expressions[expression];
      if (src === undefined) {
        throw new Error(`character has no expression "${expression}"`);
      }
      // An expression blinks only if it has an eyes-shut image.
      const blink = portrait.blink;
      const closedSource = blink?.closed[expression];
      let eyesSource: unknown;
      if (blink && closedSource !== undefined) {
        const phase = eyes(blink);
        eyesSource = phase === 'closed' ? closedSource : phase === 'half' ? blink.half?.[expression] : undefined;
      }
      const overlay = blink?.overlay === true;
      const layers: Layer[] = [
        {
          id: `${id}.portrait`,
          transform: extractTransform({}),
          opacity: 1,
          content: { type: 'image', asset: resolveAsset(eyesSource !== undefined && !overlay ? eyesSource : src) },
        },
      ];
      if (eyesSource !== undefined && overlay) {
        layers.push({
          id: `${id}.eyes`,
          transform: extractTransform({}),
          opacity: 1,
          content: { type: 'image', asset: resolveAsset(eyesSource) },
        });
      }
      const mouthSource = mouth && portrait.lipSync
        ? mouth === 'closed'
          ? portrait.lipSync.closed
          : portrait.lipSync[mouth as 'a' | 'i' | 'u' | 'e' | 'o']
        : undefined;
      if (mouthSource !== undefined) {
        layers.push({
          id: `${id}.mouth`,
          transform: extractTransform({}),
          opacity: 1,
          content: { type: 'image', asset: resolveAsset(mouthSource) },
        });
      }
      content = {
        type: 'group',
        layers,
      };
    }
  } else if (node.type === 'dialogue') {
    const view = resolveCharacterView(props.character);
    const character = resolveReference(view?.props.character) as
      | {
          portrait?: Record<string, unknown>;
          subtitle?: Record<string, unknown>;
        }
      | null;
    if (!character) {
      throw new Error('<Dialogue> requires a declared character');
    }
    const layers: Layer[] = [];
    layers.push(
      buildLayer(
        {
          type: 'text',
          props: { ...character.subtitle, id: `${id}.subtitle`, children: props.text },
          children: [],
        },
        `${path}.subtitle`,
        context,
        audio,
      ),
    );
    content = { type: 'group', layers };
  } else if (node.type === 'text') {
    const maxWidth = props.maxWidth;
    content = {
      type: 'text',
      text: extractText(props.children),
      style: (props.style as TextStyle | undefined) ?? {},
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
    ? { type: 'solid', color: value }
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

function extractEffects(props: Record<string, unknown>): Layer['effects'] | undefined {
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
  if (blur === 0 && !shadow && !glow) return undefined;
  return {
    blur,
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
    fps,
    originSec,
    rangeStartSec,
    rangeEndSec,
    characterViewOverrides: context.characterViewOverrides,
  };
}

function collectCharacterViewOverrides(node: HostNode, context: WalkContext): void {
  const childContext = node.type === 'sequence' ? childSequenceContext(node, context) : context;
  if (!childContext) {
    return;
  }
  if (node.type === 'dialogue') {
    const view = resolveCharacterView(node.props.character);
    if (view) {
      context.characterViewOverrides.set(view, node.props);
    }
  }
  for (const child of node.children) {
    collectCharacterViewOverrides(child, childContext);
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
): Layer[] {
  if (!HOST_TYPES.has(node.type)) {
    throw new Error(`unsupported element <${node.type}>; use Celesta's built-in components`);
  }
  if (node.type === 'rawLayers') {
    return (node.props.layers as Layer[] | undefined) ?? [];
  }
  if (
    node.type === 'assets' ||
    node.type === 'asset-character' ||
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
  if (node.type === 'dialogue' && node.props.audio !== undefined) {
    collectAudioClip({ ...node.props, src: node.props.audio }, context, audio);
  }
  if (node.type === 'sequence') {
    const childContext = childSequenceContext(node, context);
    if (!childContext) {
      return [];
    }
    return [buildLayer(node, path, childContext, audio)];
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
): Layer[] {
  return node.children.flatMap((child, index) =>
    walkNode(child, `${parentPath}.${index}`, context, audio),
  );
}

export function mount(defaultExport: EntryComponent): MountedComposition {
  const { container, root } = createRoot();
  let fonts: ResolvedAsset[] = [];

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
    };
    const element = React.createElement(
      ProjectLayersContext.Provider,
      { value: project?.layers ?? null },
      React.createElement(
        ProjectTrackLayersContext.Provider,
        { value: project?.tracks ?? null },
        React.createElement(
          CompositionRuntimeContext.Provider,
          { value: runtimeValue },
          React.createElement(
            TextMetricsFontsContext.Provider,
            { value: fonts },
            React.createElement(defaultExport, {}),
          ),
        ),
      ),
    );
    HostReconciler.flushSync(() => {
      HostReconciler.updateContainer(element, root, null, null);
    });
  };

  // The first pass exists only to read <Composition>'s own props, which
  // must be static (not derived from useVideoConfig()/useCurrentFrame()/
  // <ProjectTimeline />/<ProjectTrack />) — but its children still render
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

  return {
    config,
    get fonts() { return fonts; },
    renderAt(time, project) {
      renderTree(time, project, config);
      let instance = findCompositionInstance(container);
      const nextFonts: ResolvedAsset[] = [];
      collectFonts(instance, nextFonts);
      if (JSON.stringify(nextFonts) !== JSON.stringify(fonts)) {
        fonts = nextFonts;
        // A declaration can appear after its consumer, or change this frame.
        // Reconcile again with the complete font list before emitting layers.
        renderTree(time, project, config);
        instance = findCompositionInstance(container);
      }
      const audio: AudioClipDescriptor[] = [];
      const context = rootWalkContext(config.frameRate.numerator, config.durationInFrames, time);
      for (const child of instance.children) {
        collectCharacterViewOverrides(child, context);
      }
      const layers = walkChildren(
        instance,
        'root',
        context,
        audio,
      );
      return {
        scene: {
          width: config.width,
          height: config.height,
          frameRate: config.frameRate,
          time,
          ...(fonts.length > 0 ? { fonts } : {}),
          layers,
        },
        audio,
      };
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
 * `<Composition>` tree.
 */
export function createResolver(): Resolver {
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
      const runtimeValue = runtime ? { ...runtime, preview: runtime.preview === true } : PLACEHOLDER_RUNTIME;
      const element = React.createElement(
        ProjectLayersContext.Provider,
        { value: [] },
        React.createElement(
          ProjectTrackLayersContext.Provider,
          { value: {} },
          React.createElement(
            CompositionRuntimeContext.Provider,
            { value: runtimeValue },
            React.createElement(
              TextMetricsFontsContext.Provider,
              { value: fonts },
              React.createElement(ResolverHost, { items }),
            ),
          ),
        ),
      );
      HostReconciler.flushSync(() => {
        HostReconciler.updateContainer(element, root, null, null);
      });
      const walkContext = rootWalkContext(
        runtimeValue.fps,
        runtimeValue.durationInFrames,
        runtime?.time ?? ZERO_TIME,
      );
      return container.children.map((child, index) => {
        // An unresolved name renders the empty 'rawLayers' marker; anything
        // else is a resolved component's group (possibly one that rendered
        // no layers, which is still "resolved").
        if (child.type === 'rawLayers') {
          return null;
        }
        const audio: AudioClipDescriptor[] = [];
        return walkChildren(child, `resolve.${index}`, walkContext, audio);
      });
    },
  };
}
