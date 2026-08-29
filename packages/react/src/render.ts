// Mounts a Mikan React composition through react-reconciler (reconciler.ts)
// and evaluates it into the same `Scene` JSON shape that
// `mikan_composition::Scene` deserializes on the Rust side. The mount is
// persistent across frames: cli.ts calls `renderAt` once per requested
// time against the same root, so component state and effects (to the extent
// a synchronous, un-scheduled reconciler runs them) carry across frames
// exactly as they would across re-renders in any other React host.

import * as React from 'react';

import { CompositionRuntimeContext } from './hooks';
import { ProjectLayersContext, ProjectTrackLayersContext } from './project-runtime';
import { resolveVisibleLayers } from './psd-preset';
import { resolveComponent } from './registry';
import { type HostNode, type RootContainer, HostReconciler, createRoot } from './reconciler';
import type {
  CompositionConfig,
  EvaluatedTransform,
  KeyframeAnimation,
  Layer,
  LayerContent,
  Paint,
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
  'group',
  'image',
  'rect',
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
}

export interface MountedComposition {
  readonly config: CompositionConfig;
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
  };
}

function numberOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function extractTransform(props: Record<string, unknown>): EvaluatedTransform {
  // `rawTransform`/`rawOpacity` (checked here and in buildLayer) are an
  // internal escape hatch: project-runtime.ts's <ProjectTimeline /> uses
  // them to place a resolved registry component's rendered subtree at the
  // exact transform/opacity mikan-evaluator already computed for that
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
    anchor: { x: numberOr(props.anchorX, 0.5), y: numberOr(props.anchorY, 0.5) },
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
  return { id: typeof id === 'string' && id.length > 0 ? id : path, location: { type: 'file', path } };
}

function resolveReference(value: unknown): unknown {
  return value && typeof value === 'object' && 'current' in value
    ? (value as { current?: unknown }).current
    : value;
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

  let content: LayerContent;
  if (node.type === 'group' || node.type === 'sequence') {
    content = { type: 'group', layers: walkChildren(node, path, context, audio) };
  } else if (node.type === 'image') {
    content = { type: 'image', asset: resolveAsset(props.src) };
  } else if (node.type === 'character-view') {
    const character = resolveReference(props.character) as
      | {
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
              }
            | {
                type: 'psd';
                src: unknown;
                layers?: string[] | string;
                lipSync?: {
                  a: string;
                  i: string;
                  u: string;
                  e: string;
                  o: string;
                  closed?: string;
                };
              };
        }
      | null;
    const portrait = character?.portrait;
    if (!portrait) {
      throw new Error('<CharacterView> requires a character with a portrait');
    }
    const mouth = typeof props.mouth === 'string' ? props.mouth : undefined;
    if (portrait.type === 'psd') {
      const selectedLayer = mouth && portrait.lipSync
        ? mouth === 'closed'
          ? portrait.lipSync.closed
          : portrait.lipSync[mouth as 'a' | 'i' | 'u' | 'e' | 'o']
        : undefined;
      const mouthLayers = portrait.lipSync ? Object.values(portrait.lipSync) : [];
      const visibleLayers = Array.isArray(portrait.layers)
        ? portrait.layers
        : typeof portrait.layers === 'string'
          ? resolveVisibleLayers(portrait.layers)
          : [];
      content = {
        type: 'psd',
        asset: resolveAsset(portrait.src),
        ...(visibleLayers.length ? { visibleLayers } : {}),
        ...(selectedLayer ? { enabledLayers: [selectedLayer] } : {}),
        ...(selectedLayer
          ? { disabledLayers: [...new Set(mouthLayers.filter((layer) => layer !== selectedLayer))] }
          : {}),
      };
    } else {
      const expression = typeof props.expression === 'string' ? props.expression : portrait.defaultExpression;
      const src = portrait.expressions[expression];
      if (src === undefined) {
        throw new Error(`character has no expression "${expression}"`);
      }
      const layers: Layer[] = [
        {
          id: `${id}.portrait`,
          transform: extractTransform({}),
          opacity: 1,
          content: { type: 'image', asset: resolveAsset(src) },
        },
      ];
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
  } else if (node.type === 'text') {
    const maxWidth = props.maxWidth;
    content = {
      type: 'text',
      text: extractText(props.children),
      style: (props.style as TextStyle | undefined) ?? {},
      ...(typeof maxWidth === 'number' ? { maxWidth } : {}),
    };
  } else if (node.type === 'rect') {
    const fill = typeof props.fill === 'string' ? ({ type: 'solid', color: props.fill } as Paint) : undefined;
    const strokeColor = typeof props.stroke === 'string' ? props.stroke : undefined;
    const strokeWidth = numberOr(props.strokeWidth, 0);
    const stroke: Stroke | undefined =
      strokeColor && strokeWidth > 0 ? { paint: { type: 'solid', color: strokeColor }, width: strokeWidth } : undefined;
    content = {
      type: 'rect',
      width: numberOr(props.width, 0),
      height: numberOr(props.height, 0),
      ...(fill ? { fill } : {}),
      ...(stroke ? { stroke } : {}),
      cornerRadius: numberOr(props.cornerRadius, 0),
    };
  } else if (node.type === 'video') {
    // A React <Video> plays synced to the enclosing sequence chain's own
    // clock from its frame 0: `context.time` here *is* the video's local
    // time. `sourceTimeSeconds` — the only field GpuRenderer actually reads
    // to seek/decode — is `startFrom` plus local time scaled by
    // `playbackRate`, mirroring how mikan-evaluator derives it for a
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

  return { id, transform, opacity, content };
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
  };
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
    throw new Error(`unsupported element <${node.type}>; use Mikan's built-in components`);
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
  if (node.type === 'sequence') {
    const childContext = childSequenceContext(node, context);
    if (!childContext) {
      return [];
    }
    return [buildLayer(node, path, childContext, audio)];
  }
  return [buildLayer(node, path, context, audio)];
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
          React.createElement(defaultExport, {}),
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

  return {
    config,
    renderAt(time, project) {
      renderTree(time, project, config);
      const instance = findCompositionInstance(container);
      const audio: AudioClipDescriptor[] = [];
      const layers = walkChildren(
        instance,
        'root',
        rootWalkContext(config.frameRate.numerator, config.durationInFrames, time),
        audio,
      );
      return {
        scene: {
          width: config.width,
          height: config.height,
          frameRate: config.frameRate,
          time,
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
    resolve(items, runtime) {
      const runtimeValue = runtime ?? PLACEHOLDER_RUNTIME;
      const element = React.createElement(
        ProjectLayersContext.Provider,
        { value: [] },
        React.createElement(
          ProjectTrackLayersContext.Provider,
          { value: {} },
          React.createElement(
            CompositionRuntimeContext.Provider,
            { value: runtimeValue },
            React.createElement(ResolverHost, { items }),
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
