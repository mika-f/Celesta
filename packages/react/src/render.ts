// Mounts a Celesta React composition through react-reconciler (reconciler.ts)
// and evaluates it into the same `Scene` JSON shape that
// `celesta_composition::Scene` deserializes on the Rust side. The mount is
// persistent across frames: cli.ts calls `renderAt` once per requested
// time against the same root, so component state and effects (which run
// before each synchronous render returns) carry across frames exactly as
// they would across re-renders in any other React host.

import * as React from 'react';

import { isRemoteUrl } from './entry-dir';
import { CompositionRuntimeContext, CompositionSelectionContext, RerenderRequestContext, RootRuntimeContext, resolveTextLanguage } from './hooks';
import { TextMetricsFontsContext, withTextLanguage } from './text-measure';
import { flattenTextContent, withTextRuns } from './rich-text';
import { ProjectLayersContext, ProjectTrackLayersContext } from './hooks';
import { type HostWalk, hasHostElementPreparation, hostElement } from './host-elements';
import { instancesContent } from './instances';
import { type StaticsFrame, StaticSubtrees } from './static';
import { resolveComponent } from './registry';
import { type HostNode, type RootContainer, createRoot } from './reconciler';
import { type LayerSourceEntry, sourceChain } from './source-chain';
import { checkCue } from './subtitles';
import type {
  BlendMode,
  Clip,
  CompositionConfig,
  CompositionInfo,
  EvaluatedTransform,
  GroupMask,
  KeyframeAnimation,
  Layer,
  LayerContent,
  LayerFilter,
  LineDash,
  MaskMode,
  Paint,
  PathCommand,
  ResolvedAsset,
  Scene,
  ShaderSource,
  Stroke,
  TextStyle,
  Time,
} from './scene';
import { FILTER_PROPS, ShaderEffect, builtinFilter, isIdentityFilter } from './shader';
import type { BuiltinFilter } from './shader';
import type { JsonValue } from './generated/serde_json/JsonValue';
import { SECONDS_TIMESCALE, endsWithComposition, secondsFromTime, secondsToTime } from './time';

const HOST_TYPES = new Set([
  'composition',
  'assets',
  'asset-image',
  'asset-video',
  'asset-audio',
  'asset-font',
  'group',
  'mask',
  'image',
  'instances',
  'static',
  'rect',
  'path',
  'text',
  'video',
  'audio',
  'sequence',
  'freeze-frame',
  'motion-blur',
  'trail',
  'time-sample',
  'rawLayers',
  'subtitles',
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
  /** Starts the source over from its beginning past its end. Present only when set. */
  loop?: true;
  /** The soundtrack of a `<Video>`, not an `<Audio>`. Present only when set. */
  video?: true;
}

/**
 * One subtitle shown at a requested frame, from `<Subtitles>` or a
 * `<Dialogue>` line: its text and the composition seconds it is shown for,
 * cut to the enclosing `<Sequence>`s. Each frame reports the subtitles it
 * shows; the exporter merges the reports into its subtitle files.
 */
export interface SubtitleDescriptor {
  text: string;
  start: number;
  end: number;
  /** The `<Subtitles lang>` the cue is in. Present only when set. */
  lang?: string;
}

/**
 * Where the walker is inside the composition's timeline. `time` is local to
 * the enclosing `<Sequence>` chain; `originSec` converts local time back to
 * composition seconds (`local + originSec`); `rangeStartSec`/`rangeEndSec`
 * bound when the current subtree can be visible or audible, in composition
 * seconds. `compositionEndSec` is where the composition ends, which a
 * `<FreezeFrame>` restores as its children's `rangeEndSec` (running past
 * its frame when that frame is past the end); `frozen` is true inside one, and
 * `idPrefix` is prepended to authored layer ids there. `openEnd` is true when
 * a frame past the composition's end is evaluated as if the video went on
 * and this window ends with the composition; `rangeEndSec` then runs past
 * the frame. `windowEndSec` is where the window itself ends, unclipped and
 * unextended, as `<Sequence>` sees it.
 */
interface WalkContext {
  lang?: string;
  time: Time;
  fps: number;
  originSec: number;
  rangeStartSec: number;
  rangeEndSec: number;
  compositionEndSec: number;
  openEnd: boolean;
  windowEndSec: number;
  frozen: boolean;
  idPrefix: string;
  /**
   * The local time `<Video>`, animated images, and host elements are drawn
   * at: `time` itself, except inside a `<MotionBlur>` sample or a `<Trail>`
   * copy, where it is the frame's moment rather than the sample's, so every
   * sample shows the frame's picture. It goes through `<Sequence>`s exactly
   * as `time` would without the samples, so it is the very value they would
   * see.
   */
  mediaTime: Time;
  /** Shared by host elements for one frame; see `HostVisit.frameState`. */
  frameState: Map<unknown, unknown>;
  /** Leave out what Rust reads back by default; see `renderAt`'s `compact`. */
  compact: boolean;
  /** Where `<Static>` layers are cached; `null` where nothing is reused. */
  statics: StaticWalk | null;
  /** Walking a `<Static>`'s children, which cannot depend on the time. */
  insideStatic: boolean;
  /** Filled with the host node behind each layer id, for `layerSource`; `null` otherwise. */
  layerNodes: Map<string, HostNode> | null;
  /** Collects the frame's subtitles; `null` where none are shown, as inside a `<FreezeFrame>`. */
  subtitles: SubtitleDescriptor[] | null;
}

interface StaticWalk {
  cache: StaticSubtrees;
  /** Emit a `<Static>` as a reference by key instead of its layers. */
  refer: boolean;
  /** The keys this frame refers to. */
  referenced: Set<number>;
}

/** Host types whose layers depend on the time, which `<Static>` cannot hold. */
const TIME_DEPENDENT_TYPES = new Set([
  'video', 'audio', 'sequence', 'freeze-frame', 'motion-blur', 'trail', 'time-sample', 'rawLayers', 'subtitles',
]);

export interface MountedComposition {
  /** Unmounts the persistent React root and runs effect cleanup. */
  dispose(): void;
  readonly config: CompositionConfig;
  readonly compositionId: string;
  readonly compositions: readonly CompositionInfo[];
  readonly fonts: readonly ResolvedAsset[];
  /**
   * Renders one exact frame against the persistent root. Audio declarations
   * are gathered from this same tree walk — an `<Audio>` behind a
   * conditional or inside out-of-window sequences contributes on exactly the
   * frames where it actually renders. With `openEnd`, a frame past the end
   * is evaluated as if the video went on: windows that reach the end stay
   * open, while `useVideoConfig()` keeps reporting the real durations.
   */
  renderAt(
    time: Time,
    project: ProjectFrame | null,
    options?: {
      openEnd?: boolean;
      /**
       * Leave out what Rust reads back by default: transform fields equal
       * to `EvaluatedTransform`'s defaults, an opacity of 1, and a rect's
       * corner radius of 0. Such a scene is only for Rust to read.
       */
      compact?: boolean;
      /**
       * Sent by the Rust bridge: emit each `<Static>` as a reference by key
       * and return the layers the bridge still needs. `decoded` is the
       * `seq` of the last frame it decoded.
       */
      statics?: { decoded: number };
      /** Draw debug guides (`useDebugGuides()`), as PNG exports with `--debug-guides` ask. */
      debugGuides?: boolean;
      /** Evaluate as the editor preview, where `useIsPreview()` is true. Exports never set it. */
      preview?: boolean;
    },
  ): { scene: Scene; audio: AudioClipDescriptor[]; subtitles: SubtitleDescriptor[]; statics?: StaticsFrame };
  /** Sweeps every frame without constructing visual layers; hooks and font measurements still run. */
  collectAudio(): AudioClipDescriptor[];
  /**
   * Sweeps frames `[startFrame, startFrame + frameCount)` as `collectAudio()`
   * does and returns the subtitles they show, each distinct one once, in the
   * order first shown.
   */
  collectSubtitles(startFrame: number, frameCount: number): SubtitleDescriptor[];
  /**
   * The JSX that produced the layer `id` in the frame at `time`, innermost
   * first (see `sourceChain`), or null when the frame has no such layer.
   * `<Static>` subtrees are walked here rather than referenced. The frame's
   * scene is discarded and the `<Static>` cache is left as it was.
   */
  layerSource(
    time: Time,
    id: string,
    resolveFile: (fileName: string) => string,
    options?: { preview?: boolean },
  ): LayerSourceEntry[] | null;
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

function rootWalkContext(
  fps: number,
  durationInFrames: number,
  time: Time,
  lang?: string,
  openEnd = false,
  compact = false,
  statics: StaticWalk | null = null,
  layerNodes: Map<string, HostNode> | null = null,
  subtitles: SubtitleDescriptor[] | null = null,
): WalkContext {
  const endSec = durationInFrames / fps;
  return {
    lang,
    time,
    fps,
    originSec: 0,
    rangeStartSec: 0,
    rangeEndSec: openEnd ? Math.max(endSec, secondsFromTime(time) + 1 / fps) : endSec,
    compositionEndSec: endSec,
    openEnd,
    windowEndSec: endSec,
    frozen: false,
    idPrefix: '',
    mediaTime: time,
    frameState: new Map(),
    compact,
    statics,
    insideStatic: false,
    layerNodes,
    subtitles,
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

/**
 * `transform` without the fields equal to Rust's `EvaluatedTransform`
 * defaults (position 0, scale 1, rotation 0, anchor 0.5), which Rust fills
 * back in. React's own default anchor is the top-left corner, so it is
 * kept.
 */
function compactTransform(transform: EvaluatedTransform): EvaluatedTransform {
  const { position, scale, rotation, anchor } = transform;
  const compact: Partial<EvaluatedTransform> = {};
  if (position.x !== 0 || position.y !== 0) compact.position = position;
  if (scale.x !== 1 || scale.y !== 1) compact.scale = scale;
  if (rotation !== 0) compact.rotation = rotation;
  if (anchor.x !== 0.5 || anchor.y !== 0.5) compact.anchor = anchor;
  return compact as EvaluatedTransform;
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

const MASK_MODES: readonly MaskMode[] = ['alpha', 'luminance'];

/** A group's `<Mask>` host child as the scene's `GroupMask`. */
function buildMask(node: HostNode, path: string, context: WalkContext, audio: AudioClipDescriptor[]): GroupMask {
  const mode = extractMaskMode(node.props.mode);
  return {
    layers: walkChildren(node, path, context, audio),
    ...(mode !== 'alpha' ? { mode } : {}),
    ...(node.props.invert === true ? { invert: true } : {}),
  };
}

function extractMaskMode(value: unknown): MaskMode {
  if (value === undefined) {
    return 'alpha';
  }
  if (!MASK_MODES.includes(value as MaskMode)) {
    throw new Error(`unknown mask mode ${JSON.stringify(value)}; expected one of ${MASK_MODES.join(', ')}`);
  }
  return value as MaskMode;
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
    frame: Math.round((secondsFromTime(context.mediaTime) + context.originSec) * context.fps),
    fps: context.fps,
    frozen: context.frozen,
    frameState: context.frameState,
    resolveAsset,
    layers: (node, path) => walkNode(node, path, context, audio),
    collectAudio: (props) => collectAudioClip(props, context, audio),
    collectSubtitle: (text) => collectSubtitle(text, 0, Infinity, context),
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

/**
 * Records the host node behind a layer id for `layerSource`. Only nodes React
 * created have a fiber to walk; a node a host element makes up (such as
 * `<Dialogue>`'s default subtitle) is left out, and `recordBuiltLayers` maps
 * its layers to the host element instead.
 */
function recordLayerNode(context: WalkContext, id: string, node: HostNode): void {
  if (node.fiber != null) {
    context.layerNodes?.set(id, node);
  }
}

/**
 * Maps every layer a host element or `rawLayers` built itself, group
 * children included, that has no node yet to `node`, the nearest node with
 * a fiber.
 */
function recordBuiltLayers(context: WalkContext, layers: readonly Layer[], node: HostNode): void {
  const { layerNodes } = context;
  if (!layerNodes || node.fiber == null) {
    return;
  }
  for (const layer of layers) {
    if (!layerNodes.has(layer.id)) {
      layerNodes.set(layer.id, node);
    }
    if (layer.content.type === 'group') {
      recordBuiltLayers(context, layer.content.layers, node);
      if (layer.content.mask) recordBuiltLayers(context, layer.content.mask.layers, node);
    }
  }
}

function buildLayer(
  node: HostNode,
  path: string,
  context: WalkContext,
  audio: AudioClipDescriptor[],
): Layer {
  const { props } = node;
  const id = typeof props.id === 'string' && props.id.length > 0 ? flatString(`${context.idPrefix}${props.id}`) : path;
  recordLayerNode(context, id, node);
  // An `<Instances>`'s x, y, and rotation are its particles', not its own.
  const fullTransform = extractTransform(
    node.type === 'instances'
      ? { ...props, x: undefined, y: undefined, rotation: undefined, anchorX: undefined, anchorY: undefined }
      : props,
  );
  const transform = context.compact ? compactTransform(fullTransform) : fullTransform;
  const opacity = typeof props.rawOpacity === 'number' ? props.rawOpacity : numberOr(props.opacity, 1);
  const blendMode = extractBlendMode(props.blendMode);
  const effects = extractEffects(props, context);

  let content: LayerContent;
  if (node.type === 'group' || node.type === 'sequence') {
    const clip = node.type === 'group' ? extractClip(props.clip) : undefined;
    let mask: GroupMask | undefined;
    // Siblings in order, so audio is collected in the order the audio-only
    // walk collects it.
    const layers = node.children.flatMap((child, index) => {
      const childPath = [path, index].join('.');
      if (node.type === 'group' && child.type === 'mask') {
        if (mask) {
          throw new Error('a <Group> takes at most one mask');
        }
        mask = buildMask(child, childPath, context, audio);
        return [];
      }
      return walkNode(child, childPath, context, audio);
    });
    content = {
      type: 'group',
      layers,
      ...(clip ? { clip } : {}),
      ...(mask ? { mask } : {}),
    };
  } else if (node.type === 'freeze-frame') {
    // Frozen children are always silent: their audio goes nowhere.
    content = { type: 'group', layers: walkChildren(node, path, childFreezeContext(node, context, id), []) };
  } else if (node.type === 'motion-blur') {
    content = { type: 'group', layers: sampleLayers(node, path, id, context, audio), motionBlur: true };
  } else if (node.type === 'trail') {
    content = { type: 'group', layers: sampleLayers(node, path, id, context, audio) };
  } else if (node.type === 'image') {
    for (const key of ['width', 'height']) {
      if (props[key] !== undefined && (typeof props[key] !== 'number' || !Number.isFinite(props[key]) || props[key] <= 0)) {
        throw new Error(`Image ${key} must be finite and positive`);
      }
    }
    if (props.fit !== undefined && props.fit !== 'contain' && props.fit !== 'cover') {
      throw new Error('Image fit must be contain or cover');
    }
    for (const key of ['startFrom', 'playbackRate']) {
      if (props[key] !== undefined && (typeof props[key] !== 'number' || !Number.isFinite(props[key]))) {
        throw new Error(`Image ${key} must be a finite number`);
      }
    }
    if (props.loop !== undefined && typeof props.loop !== 'boolean') {
      throw new Error('Image loop must be a boolean');
    }
    content = { type: 'image', asset: resolveAsset(props.src),
      ...(props.width === undefined ? {} : { width: props.width as number }),
      ...(props.height === undefined ? {} : { height: props.height as number }),
      ...(props.fit === undefined ? {} : { fit: props.fit as 'contain' | 'cover' }),
      // Whether the file is animated is not known here, so every image
      // carries the time, as a <Video> does; Rust drops it from still
      // images. A <Static> subtree cannot depend on the time.
      ...(context.insideStatic ? {} : {
        animation: {
          sourceTimeSeconds: numberOr(props.startFrom, 0) + secondsFromTime(context.mediaTime) * numberOr(props.playbackRate, 1),
          loop: props.loop !== false,
        },
      }),
    };
  } else if (hostElement(node.type)?.content) {
    content = hostElement(node.type)!.content!(node, hostWalk(id, context, audio));
    if (content.type === 'group') {
      recordBuiltLayers(context, content.layers, node);
      if (content.mask) recordBuiltLayers(context, content.mask.layers, node);
    }
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
  } else if (node.type === 'instances') {
    content = instancesContent(props, resolveAsset);
  } else if (node.type === 'rect') {
    const fill = toPaint(props.fill);
    const strokePaint = toPaint(props.stroke);
    const strokeWidth = numberOr(props.strokeWidth, 0);
    const stroke: Stroke | undefined =
      strokePaint && strokeWidth > 0 ? { paint: strokePaint, width: strokeWidth } : undefined;
    const cornerRadius = numberOr(props.cornerRadius, 0);
    content = {
      type: 'rect',
      width: numberOr(props.width, 0),
      height: numberOr(props.height, 0),
      ...(fill ? { fill } : {}),
      ...(stroke ? { stroke } : {}),
      // Rust reads a missing corner radius as 0.
      ...(cornerRadius !== 0 || !context.compact ? { cornerRadius } : {}),
    } as LayerContent;
  } else if (node.type === 'path') {
    const fill = toPaint(props.fill);
    const strokePaint = toPaint(props.stroke);
    const strokeWidth = numberOr(props.strokeWidth, 0);
    const miterLimit = props.miterLimit;
    if (miterLimit !== undefined && (typeof miterLimit !== 'number' || !Number.isFinite(miterLimit) || miterLimit < 1)) {
      throw new Error('<Path> miterLimit must be a finite number of at least 1');
    }
    const lineDash = extractLineDash(props.dash, props.dashOffset);
    content = {
      type: 'path',
      commands: extractPathCommands(props.commands),
      ...(fill ? { fill } : {}),
      ...(strokePaint && strokeWidth > 0 ? { stroke: { paint: strokePaint, width: strokeWidth } } : {}),
      ...(props.cap && props.cap !== 'butt' ? { lineCap: extractOneOf(props.cap, LINE_CAPS, 'cap') } : {}),
      ...(props.join && props.join !== 'miter' ? { lineJoin: extractOneOf(props.join, LINE_JOINS, 'join') } : {}),
      ...(miterLimit !== undefined && miterLimit !== 4 ? { miterLimit } : {}),
      ...(lineDash ? { lineDash } : {}),
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
    const localTime = context.mediaTime;
    content = {
      type: 'video',
      asset: resolveAsset(props.src),
      timing: {
        localTime,
        sourceStart: secondsToTime(startFrom),
        sourceTimeSeconds: startFrom + secondsFromTime(localTime) * playbackRate,
        playbackRate,
        ...(props.loop === true ? { loop: true } : {}),
      },
    };
  } else {
    throw new Error(`unreachable: unknown host node type "${node.type}"`);
  }

  // Rust reads a missing opacity as 1.
  return { id, transform, ...(opacity !== 1 || !context.compact ? { opacity } : {}),
    ...(blendMode !== 'normal' ? { blendMode } : {}),
    ...(effects ? { effects } : {}), content } as Layer;
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

/** A path's `dash` and `dashOffset`, or none for a solid stroke. */
function extractLineDash(dash: unknown, offset: unknown): LineDash | undefined {
  if (offset !== undefined && (typeof offset !== 'number' || !Number.isFinite(offset))) {
    throw new Error('<Path> dashOffset must be a finite number');
  }
  if (dash === undefined) {
    return undefined;
  }
  if (!Array.isArray(dash) || !dash.every((length) => typeof length === 'number' && Number.isFinite(length) && length >= 0)) {
    throw new Error('<Path> dash must be an array of finite numbers of at least 0');
  }
  if (dash.length === 0) {
    return undefined;
  }
  return { pattern: [...dash], ...(offset ? { offset } : {}) };
}

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

/**
 * The layer's filters, in the order they run: the `shader` prop's, then the
 * built-in filter props' in `FILTER_PROPS` order. Props that leave the
 * pixels as they were add no filter.
 */
function extractFilters(props: Record<string, unknown>, context: WalkContext): LayerFilter[] {
  const value = props.shader;
  // A noise filter without a seed takes the frame `useCurrentFrame()` reads
  // here, at the frame's moment as media is: every <MotionBlur> sample and
  // <Trail> copy gets the frame's grain. A <Static> subtree cannot depend on it.
  const frame = context.insideStatic ? null : Math.round(secondsFromTime(context.mediaTime) * context.fps);
  // A dense copy: a hole is refused like any other value that is not a shader.
  const effects = value === undefined ? [] : Array.isArray(value) ? Array.from(value as unknown[]) : [value];
  const filters = effects.map((effect) => {
    if (!(effect instanceof ShaderEffect)) {
      throw new Error("shader must be a value returned by @celesta/shader (defineShader() or a built-in filter such as brightness()), or an array of them");
    }
    const source = ShaderEffect.source(effect);
    if (source) useShaderSource(source.source, context);
    return ShaderEffect.serialize(effect, frame);
  });
  for (const name of FILTER_PROPS) {
    if (props[name] === undefined) continue;
    const filter = ShaderEffect.serialize(builtinFilter(name, props[name] as never), frame) as BuiltinFilter;
    if (!isIdentityFilter(filter)) filters.push(filter);
  }
  return filters;
}

/** Lists `source` among the frame's shaders. */
function useShaderSource(source: ShaderSource, context: WalkContext): void {
  const sources = usedShaders(context);
  context.frameState.set(SHADER_SOURCES, sources);
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
}

function extractEffects(props: Record<string, unknown>, context: WalkContext): Layer['effects'] | undefined {
  // `rawEffects`, like `rawTransform`, carries what celesta-evaluator
  // already computed for a project timeline item; project items have no
  // custom shaders, so there are no sources to list.
  if (props.rawEffects !== undefined) return props.rawEffects as Layer['effects'];
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
  const filters = extractFilters(props, context);
  if (blur === 0 && !shadow && !glow && filters.length === 0) return undefined;
  return {
    blur,
    ...(filters.length > 0 ? { filters } : {}),
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

const BLEND_MODES: readonly BlendMode[] = [
  'normal', 'multiply', 'screen', 'overlay', 'add', 'difference',
  'darken', 'lighten', 'colorDodge', 'colorBurn', 'hardLight', 'softLight', 'exclusion',
  'hue', 'saturation', 'color', 'luminosity',
];

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
  const hasDuration = typeof props.durationInFrames === 'number' && Number.isFinite(props.durationInFrames);
  const endLocalSec = hasDuration
    ? (fromFrames + Math.max(0, props.durationInFrames as number)) / fps
    : context.rangeEndSec - context.originSec;
  const windowEndSec = hasDuration ? context.originSec + endLocalSec : context.windowEndSec;
  // Past the composition's end, a window that ends with the composition
  // stays open; one that ends later keeps its own end. <Sequence> in
  // components.ts decides the same way.
  const openEnd = context.openEnd && endsWithComposition(windowEndSec, context.compositionEndSec, fps);
  const windowEndLocalSec = openEnd ? context.rangeEndSec - context.originSec : endLocalSec;
  const localSec = secondsFromTime(context.time);
  if (localSec < startLocalSec || localSec >= windowEndLocalSec) {
    return null;
  }
  const originSec = context.originSec + startLocalSec;
  const rangeStartSec = Math.max(context.rangeStartSec, originSec);
  const rangeEndSec = Math.min(context.rangeEndSec, context.originSec + windowEndLocalSec);
  if (rangeEndSec <= rangeStartSec) {
    return null;
  }
  const time = secondsToTime(localSec - startLocalSec);
  return {
    time,
    lang: context.lang,
    fps,
    originSec,
    rangeStartSec,
    rangeEndSec,
    compositionEndSec: context.compositionEndSec,
    openEnd,
    windowEndSec,
    frozen: context.frozen,
    idPrefix: context.idPrefix,
    mediaTime: context.mediaTime === context.time ? time : sequenceMediaTime(context.mediaTime, startLocalSec),
    frameState: context.frameState,
    compact: context.compact,
    statics: context.statics,
    insideStatic: context.insideStatic,
    layerNodes: context.layerNodes,
    subtitles: context.subtitles,
  };
}

/**
 * The walk context for a `<FreezeFrame>`'s children: the root clock at its
 * `frame`, with the root's range rather than the enclosing one, so a freeze
 * inside a short `<Sequence>` still opens the sequences its frame is in.
 * A frame at or past the composition's end is evaluated as if the video
 * went on (`openEnd`), as `<FreezeFrame>` in components.ts decides, so a
 * short composition draws the same frame of the tree as a long one would.
 * Authored ids inside are prefixed with the freeze layer's id.
 */
function childFreezeContext(node: HostNode, context: WalkContext, layerId: string): WalkContext {
  const time = secondsToTime(numberOr(node.props.frame, 0) / context.fps);
  const frozenSec = secondsFromTime(time);
  const openEnd = frozenSec >= context.compositionEndSec;
  return {
    time,
    lang: context.lang,
    fps: context.fps,
    originSec: 0,
    rangeStartSec: 0,
    rangeEndSec: openEnd ? frozenSec + 1 / context.fps : context.compositionEndSec,
    compositionEndSec: context.compositionEndSec,
    openEnd,
    windowEndSec: context.compositionEndSec,
    frozen: true,
    idPrefix: `${layerId}/`,
    // A freeze's moment is its own frame, not one near the frame drawn.
    mediaTime: time,
    frameState: context.frameState,
    compact: context.compact,
    statics: context.statics,
    insideStatic: context.insideStatic,
    layerNodes: context.layerNodes,
    // A frozen picture shows no subtitles of its own, as it plays no audio.
    subtitles: null,
  };
}

/**
 * A sample's `mediaTime` inside a `<Sequence>` starting `startLocalSec`
 * into its parent: as `childSequenceContext` shifts `time`, except that a
 * sequence the sample has entered before the frame has (the sample being
 * later) starts at 0.
 */
function sequenceMediaTime(mediaTime: Time, startLocalSec: number): Time {
  const seconds = secondsFromTime(mediaTime) - startLocalSec;
  return secondsToTime(Math.max(0, seconds));
}

/**
 * The walk context for a 'time-sample''s children: its moment, inside a
 * copy (`frozen`) unless it is a `<Trail>`'s live children. `mediaTime`
 * stays the frame's.
 */
function childSampleContext(node: HostNode, context: WalkContext): WalkContext {
  if (node.props.live === true) {
    return context;
  }
  return { ...context, time: node.props.time as Time, frozen: true };
}

/**
 * The sample of a 'motion-blur' or 'trail' whose audio plays and whose
 * subtitles show: a `<Trail>`'s
 * live children, or the `<MotionBlur>` moment nearest the frame (the
 * earlier of two as near).
 */
function audibleSample(node: HostNode, context: WalkContext): HostNode | undefined {
  const now = secondsFromTime(context.time);
  let nearest: HostNode | undefined;
  let distance = Infinity;
  for (const sample of node.children) {
    if (sample.props.live === true) return sample;
    const from = Math.abs(secondsFromTime(sample.props.time as Time) - now);
    if (from < distance) {
      nearest = sample;
      distance = from;
    }
  }
  return nearest;
}

/**
 * The layers of a 'motion-blur''s or 'trail''s samples: each moment as a
 * group of its own (a `<Trail>` copy's at its opacity), and a `<Trail>`'s
 * live children as they are. Every sample's children are walked with the
 * paths they would have without the samples, so a layer has the same id in
 * every moment and a `<Video>` decodes once.
 */
function sampleLayers(
  node: HostNode,
  path: string,
  id: string,
  context: WalkContext,
  audio: AudioClipDescriptor[],
): Layer[] {
  const audible = audibleSample(node, context);
  const layers: Layer[] = [];
  node.children.forEach((sample, index) => {
    // Audio and subtitles come from one sample only.
    const sampleContext = childSampleContext(sample, context);
    const children = sample === audible
      ? walkChildren(sample, path, sampleContext, audio)
      : walkChildren(sample, path, { ...sampleContext, subtitles: null }, []);
    if (sample.props.live === true) {
      layers.push(...children);
      return;
    }
    const sampleId = [id, 'sample', index].join('.');
    recordLayerNode(context, sampleId, sample);
    const opacity = numberOr(sample.props.opacity, 1);
    layers.push({
      id: sampleId,
      transform: identityTransform(context),
      ...(opacity !== 1 || !context.compact ? { opacity } : {}),
      content: { type: 'group', layers: children },
    } as Layer);
  });
  return layers;
}

/** The transform of a group the walk adds around layers, which leaves them where they are. */
function identityTransform(context: WalkContext): EvaluatedTransform {
  const identity: EvaluatedTransform = {
    position: { x: 0, y: 0 },
    scale: { x: 1, y: 1 },
    rotation: 0,
    anchor: { x: 0, y: 0 },
  };
  return context.compact ? compactTransform(identity) : identity;
}

/** The walk context for `node`'s children. */
function childWalkContext(node: HostNode, context: WalkContext): WalkContext | null {
  return node.type === 'sequence'
    ? childSequenceContext(node, context)
    : node.type === 'freeze-frame'
      ? childFreezeContext(node, context, '')
      : node.type === 'time-sample'
        ? childSampleContext(node, context)
        : context;
}

/** Runs registered host elements' `prepare` over the nodes this frame shows. */
function prepareHostElements(node: HostNode, context: WalkContext): void {
  const childContext = childWalkContext(node, context);
  if (!childContext) {
    return;
  }
  hostElement(node.type)?.prepare?.(node, context);
  for (const child of node.children) {
    prepareHostElements(child, childContext);
  }
}

function collectAudioClip(
  props: Record<string, unknown>,
  context: WalkContext,
  results: AudioClipDescriptor[],
  video = false,
): void {
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
    ...(props.loop === true ? { loop: true } : {}),
    ...(video ? { video: true } : {}),
  });
}

/**
 * Reports `text` as shown from local second `startLocalSec` to `endLocalSec`
 * of the enclosing `<Sequence>`, cut to its window, in language `lang`.
 */
function collectSubtitle(
  text: string,
  startLocalSec: number,
  endLocalSec: number,
  context: WalkContext,
  lang?: string,
): void {
  if (!context.subtitles || text.trim() === '') {
    return;
  }
  const start = Math.max(context.rangeStartSec, context.originSec + startLocalSec);
  const end = Math.min(context.rangeEndSec, context.originSec + endLocalSec);
  if (end > start) {
    context.subtitles.push({ text, start, end, ...(lang ? { lang } : {}) });
  }
}

// A BCP 47 language tag's shape: letters and digits in hyphen-separated
// subtags. It also names the subtitle file, so nothing else is allowed.
const SUBTITLE_LANGUAGE = /^[A-Za-z0-9]{1,8}(?:-[A-Za-z0-9]{1,8})*$/;

/** Reports the `<Subtitles>` cues shown at this frame. */
function collectSubtitleCues(props: Record<string, unknown>, context: WalkContext): void {
  if (!context.subtitles) {
    return;
  }
  const { cues, lang } = props;
  if (!Array.isArray(cues)) {
    throw new Error('<Subtitles> requires a `cues` array');
  }
  if (lang !== undefined && (typeof lang !== 'string' || (lang !== '' && !SUBTITLE_LANGUAGE.test(lang)))) {
    throw new Error('<Subtitles> requires `lang` to be a language tag such as "en" or "pt-BR"');
  }
  const language = lang as string | undefined;
  const localSec = secondsFromTime(context.time);
  cues.forEach((cue: unknown, index) => {
    checkCue(cue, index, '<Subtitles>');
    const startLocalSec = cue.from / context.fps;
    const endLocalSec = (cue.from + cue.durationInFrames) / context.fps;
    if (localSec >= startLocalSec && localSec < endLocalSec) {
      collectSubtitle(cue.text, startLocalSec, endLocalSec, context, language);
    }
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
  if (context.insideStatic && (element || TIME_DEPENDENT_TYPES.has(node.type))) {
    throw new Error(`<Static> cannot contain <${node.type}>`);
  }
  if (node.type === 'static') {
    return audioOnly ? [] : [staticLayer(node, path, context, audio)];
  }
  if (element) {
    if (element.audio || element.subtitles) {
      const walk = hostWalk(path, context, audio);
      element.audio?.(node, walk);
      element.subtitles?.(node, walk);
    }
    return audioOnly || !element.content ? [] : [buildLayer(node, path, context, audio)];
  }
  if (!HOST_TYPES.has(node.type)) {
    throw new Error(`unsupported element <${node.type}>; use Celesta's built-in components`);
  }
  if ((node.type === 'group' || node.type === 'sequence') && node.props.lang !== undefined) {
    context = { ...context, lang: resolveTextLanguage(node.props.lang, context.lang) };
  }
  if (node.type === 'rawLayers') {
    if (audioOnly) {
      return [];
    }
    const layers = inheritTextLanguage((node.props.layers as Layer[] | undefined) ?? [], context.lang);
    recordBuiltLayers(context, layers, node);
    return layers;
  }
  if (node.type === 'mask') {
    // A group's mask is taken out by buildLayer; anywhere else is misplaced.
    // The audio-only walk does not check placement, the visual one does.
    if (audioOnly) {
      return walkChildren(node, path, context, audio, true);
    }
    throw new Error('`<Mask>` must be a direct child of `<Group>`');
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
  if (node.type === 'subtitles') {
    collectSubtitleCues(node.props, context);
    return [];
  }
  if (node.type === 'audio') {
    // <Audio> contributes no visual Layer (there is no LayerContent audio
    // variant — audio is a separate top-level AudioGraph); it is collected
    // during this same walk instead.
    collectAudioClip(node.props, context, audio);
    return [];
  }
  if (node.type === 'video') {
    // A <Video> plays its soundtrack exactly as an <Audio> with the same
    // `src`, `startFrom`, `playbackRate`, `volume` and `muted` would, so
    // sound and picture share one clock. A file without an audio stream
    // mixes as silence.
    collectAudioClip(node.props, context, audio, true);
    if (audioOnly) {
      return [];
    }
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
    if (node.type === 'motion-blur' || node.type === 'trail') {
      // One sample plays, as when its layers are walked.
      const sample = audibleSample(node, context);
      return sample ? walkChildren(sample, path, childSampleContext(sample, context), audio, true) : [];
    }
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

/**
 * A `<Static>`: a group layer whose children are walked only when something
 * they depend on changed, and which a frame asked for with `statics` sends
 * as a reference to layers the bridge already has.
 */
function staticLayer(node: HostNode, path: string, context: WalkContext, audio: AudioClipDescriptor[]): Layer {
  const { statics } = context;
  recordLayerNode(context, path, node);
  const key = { path, lang: context.lang, idPrefix: context.idPrefix, compact: context.compact };
  // The children are walked with a frame state of their own, so the custom
  // shaders they use are known and can be listed again on frames that reuse
  // their layers. No host element, the other user of frame state, can be
  // inside.
  const walk = () => {
    const childContext: WalkContext = {
      ...context,
      insideStatic: true,
      // A definition holds its nested `<Static>`s' layers, never references.
      statics: statics && { ...statics, refer: false },
      frameState: new Map(),
    };
    const layers = walkChildren(node, path, childContext, audio);
    return { layers, shaders: usedShaders(childContext) };
  };
  const entry = statics ? (statics.cache.lookup(node, key) ?? statics.cache.store(node, walk(), key)) : null;
  const walked = entry ? undefined : walk();
  for (const shader of entry ? entry.shaders : walked!.shaders) useShaderSource(shader, context);
  let content: LayerContent;
  if (entry && statics?.refer) {
    statics.referenced.add(entry.key);
    content = { type: 'staticRef', key: entry.key };
  } else {
    content = { type: 'group', layers: entry ? entry.layers : walked!.layers };
  }
  return {
    id: path,
    transform: identityTransform(context),
    ...(context.compact ? {} : { opacity: 1 }),
    content,
  } as Layer;
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
      return {
        ...layer,
        content: {
          ...content,
          layers: inheritTextLanguage(content.layers, lang),
          ...(content.mask ? { mask: { ...content.mask, layers: inheritTextLanguage(content.mask.layers, lang) } } : {}),
        },
      };
    }
    return layer;
  });
}

export function mount(defaultExport: EntryComponent, compositionId?: string): MountedComposition {
  let selection: string | null = null;
  const root = createRoot();
  const { container } = root;
  let fonts: ResolvedAsset[] = [];
  let rerenderRequested = false;
  const requestRerender = () => {
    rerenderRequested = true;
  };

  const renderTree = (
    time: Time,
    project: ProjectFrame | null,
    config: CompositionConfig | PlaceholderConfig,
    openEnd = false,
    debugGuides = false,
    preview = false,
  ) => {
    const runtimeValue = {
      time,
      width: config.width,
      height: config.height,
      fps: config.frameRate.numerator,
      durationInFrames: config.durationInFrames,
      preview,
      ...(debugGuides ? { debugGuides } : {}),
      lang: config.lang,
      openEnd,
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
                React.createElement(CompositionSelectionContext.Provider, { value: selection },
                  React.createElement(defaultExport, {})),
              ),
            ),
          ),
        ),
      ),
    );
    root.render(element);
  };

  // IDs and size/clock props must be static, independent of the placeholder
  // runtime or project layers. Discover roots without mounting their children.
  renderTree(ZERO_TIME, { layers: [], tracks: {} }, PLACEHOLDER_CONFIG);
  const roots = container.children;
  if (roots.length === 0 || roots.some((node) => node.type !== 'composition')) {
    throw new Error("the entry module's default export must render root <Composition> elements");
  }
  const ids = new Set<string>();
  const compositions = roots.map((node): CompositionInfo => {
    const id = node.props.id;
    if ((id !== undefined && (typeof id !== 'string' || id.trim().length === 0)) ||
        (roots.length > 1 && id === undefined)) {
      throw new Error('each <Composition> requires a non-empty string `id` when registering multiple compositions');
    }
    const resolvedId = (id as string | undefined) ?? 'default';
    if (ids.has(resolvedId)) throw new Error(`duplicate <Composition> id: ${resolvedId}`);
    ids.add(resolvedId);
    return { id: resolvedId, ...readCompositionConfig(node) };
  });
  const selected = compositionId === undefined ? compositions[0] : compositions.find((item) => item.id === compositionId);
  if (!selected) throw new Error(`unknown composition id: ${compositionId}; available: ${[...ids].join(', ')}`);
  selection = selected.id;
  const { id: _id, ...config } = selected;
  renderTree(ZERO_TIME, { layers: [], tracks: {} }, config);
  const compositionInstance = findCompositionInstance(container);
  const initialFonts: ResolvedAsset[] = [];
  collectFonts(compositionInstance, initialFonts);
  fonts = initialFonts;
  const staticSubtrees = new StaticSubtrees();

  /** Renders the tree for `time`, again if fonts or refs changed, and returns the `<Composition>`. */
  const settleTree = (
    time: Time,
    project: ProjectFrame | null,
    openEnd: boolean,
    debugGuides: boolean,
    preview: boolean,
  ) => {
    rerenderRequested = false;
    renderTree(time, project, config, openEnd, debugGuides, preview);
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
      renderTree(time, project, config, openEnd, debugGuides, preview);
      instance = findCompositionInstance(container);
    }
    return instance;
  };

  const renderFrame = (
    time: Time,
    project: ProjectFrame | null,
    audioOnly = false,
    openEnd = false,
    compact = false,
    statics?: { decoded: number },
    debugGuides = false,
    preview = false,
  ) => {
    const instance = settleTree(time, project, openEnd, debugGuides, preview);
    const audio: AudioClipDescriptor[] = [];
    const subtitles: SubtitleDescriptor[] = [];
    const context = rootWalkContext(
      config.frameRate.numerator,
      config.durationInFrames,
      time,
      resolveTextLanguage(instance.props.lang),
      openEnd,
      compact,
      audioOnly ? null : { cache: staticSubtrees, refer: statics !== undefined, referenced: new Set() },
      null,
      subtitles,
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
    if (!audioOnly) staticSubtrees.sweep(container.children);
    const staticsFrame = statics && context.statics
      ? staticSubtrees.frame(context.statics.referenced, statics.decoded)
      : undefined;
    return {
      ...(staticsFrame ? { statics: staticsFrame } : {}),
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
      subtitles,
    };
  };

  return {
    dispose() { root.render(null); },
    config,
    compositionId: selected.id,
    compositions,
    get fonts() { return fonts; },
    renderAt: (time, project, options) =>
      renderFrame(
        time,
        project,
        false,
        options?.openEnd === true,
        options?.compact === true,
        options?.statics,
        options?.debugGuides === true,
        options?.preview === true,
      ),
    collectAudio() {
      const audio: AudioClipDescriptor[] = [];
      for (let frame = 0; frame < config.durationInFrames; frame++) {
        const time = { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator };
        for (const clip of renderFrame(time, null, true).audio) audio.push(clip);
      }
      return audio;
    },
    collectSubtitles(startFrame, frameCount) {
      const shown = new Map<string, SubtitleDescriptor>();
      for (let frame = startFrame; frame < startFrame + frameCount; frame++) {
        const time = { value: frame * config.frameRate.denominator, timescale: config.frameRate.numerator };
        for (const subtitle of renderFrame(time, null, true).subtitles) {
          const key = JSON.stringify([subtitle.lang ?? null, subtitle.text, subtitle.start, subtitle.end]);
          if (!shown.has(key)) shown.set(key, subtitle);
        }
      }
      return [...shown.values()];
    },
    layerSource(time, id, resolveFile, options) {
      const instance = settleTree(time, null, false, false, options?.preview === true);
      const layerNodes = new Map<string, HostNode>();
      // No statics: a <Static> is walked, not referenced, so its layers are
      // reachable, and its cached layers and sequence numbers stay as they
      // were (no sweep either). The scene itself is thrown away.
      const context = rootWalkContext(
        config.frameRate.numerator,
        config.durationInFrames,
        time,
        resolveTextLanguage(instance.props.lang),
        false,
        false,
        null,
        layerNodes,
      );
      if (hasHostElementPreparation()) {
        for (const child of instance.children) {
          prepareHostElements(child, context);
        }
      }
      walkChildren(instance, 'root', context, []);
      const node = layerNodes.get(id);
      return node ? sourceChain(node, resolveFile) : null;
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

/** What one `Resolver.resolve` call produced. */
export interface ComponentResolutions {
  /** One outcome per request, in order. */
  components: ComponentResolution[];
  /**
   * The custom shaders the resolved layers use, each once; absent when they
   * use none. The caller merges them into the scene the layers go into.
   */
  shaders?: ShaderSource[];
}

export interface Resolver {
  /**
   * Renders each request's registered component against a dedicated
   * persistent root (separate from the composition's, so hook state in
   * resolved components survives across calls exactly as it does for the
   * main tree), returning the layers each one produced — or null for a
   * name with no matching registerComponent() call — and the shader sources
   * those layers use. `runtime`, when given,
   * is what those components' `useCurrentFrame()`/`useVideoConfig()` see;
   * without one they fall back to the placeholder frame-0 runtime.
   */
  resolve(
    items: readonly ComponentResolutionRequest[],
    runtime?: ResolutionRuntime,
    fonts?: readonly ResolvedAsset[],
  ): ComponentResolutions;
}

/**
 * Standalone component resolver used by the editor preview: resolves
 * individual `registerComponent()` names without going through a
 * `<Composition>` tree. `lang` supplies that composition's text language.
 */
export function createResolver(lang?: string): Resolver {
  const root = createRoot();
  const { container } = root;

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
      root.render(element());
      if (rerenderRequested) {
        // A ref read during render is attached only by the commit.
        root.render(element());
      }
      const walkContext = rootWalkContext(
        runtimeValue.fps,
        runtimeValue.durationInFrames,
        runtime?.time ?? ZERO_TIME,
        runtimeValue.lang,
      );
      const components = container.children.map((child, index) => {
        // An unresolved name renders the empty 'rawLayers' marker; anything
        // else is a resolved component's group (possibly one that rendered
        // no layers, which is still "resolved").
        if (child.type === 'rawLayers') {
          return null;
        }
        const audio: AudioClipDescriptor[] = [];
        return walkChildren(child, `resolve.${index}`, walkContext, audio);
      });
      // Every item shares `walkContext`, so this lists each source once.
      const shaders = usedShaders(walkContext);
      return shaders.length > 0 ? { components, shaders } : { components };
    },
  };
}
