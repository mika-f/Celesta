// Mounts a Mikan React composition through react-reconciler (reconciler.ts)
// and evaluates it into the same `Scene` JSON shape that
// `mikan_composition::Scene` deserializes on the Rust side. The mount is
// persistent across frames: cli.ts calls `renderSceneAt` once per requested
// time against the same root, so component state and effects (to the extent
// a synchronous, un-scheduled reconciler runs them) carry across frames
// exactly as they would across re-renders in any other React host.

import * as React from 'react';

import { CompositionRuntimeContext } from './hooks';
import { ProjectLayersContext, ProjectTrackLayersContext } from './project-runtime';
import { type HostNode, type RootContainer, HostReconciler, createRoot } from './reconciler';
import type {
  CompositionConfig,
  EvaluatedTransform,
  Layer,
  LayerContent,
  ResolvedAsset,
  Scene,
  TextStyle,
  Time,
} from './scene';

const HOST_TYPES = new Set(['composition', 'group', 'image', 'text', 'rawLayers']);
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

export type EntryComponent = (props: Record<string, unknown>) => React.ReactNode;

export interface ProjectFrame {
  layers: Layer[];
  tracks: Record<string, Layer[]>;
}

export interface MountedComposition {
  readonly config: CompositionConfig;
  renderAt(time: Time, project: ProjectFrame | null): Scene;
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
  if (typeof src !== 'string' || src.length === 0) {
    throw new Error('components with asset content require a non-empty `src` prop');
  }
  return { id: src, location: { type: 'file', path: src } };
}

function buildLayer(node: HostNode, path: string): Layer {
  const { props } = node;
  const id = typeof props.id === 'string' && props.id.length > 0 ? props.id : path;
  const transform = extractTransform(props);
  const opacity = typeof props.rawOpacity === 'number' ? props.rawOpacity : numberOr(props.opacity, 1);

  let content: LayerContent;
  if (node.type === 'group') {
    content = { type: 'group', layers: walkChildren(node, path) };
  } else if (node.type === 'image') {
    content = { type: 'image', asset: resolveAsset(props.src) };
  } else if (node.type === 'text') {
    const maxWidth = props.maxWidth;
    content = {
      type: 'text',
      text: extractText(props.children),
      style: (props.style as TextStyle | undefined) ?? {},
      ...(typeof maxWidth === 'number' ? { maxWidth } : {}),
    };
  } else {
    throw new Error(`unreachable: unknown host node type "${node.type}"`);
  }

  return { id, transform, opacity, content };
}

function walkNode(node: HostNode, path: string): Layer[] {
  if (!HOST_TYPES.has(node.type)) {
    throw new Error(`unsupported element <${node.type}>; use Mikan's built-in components`);
  }
  if (node.type === 'rawLayers') {
    return (node.props.layers as Layer[] | undefined) ?? [];
  }
  return [buildLayer(node, path)];
}

function walkChildren(node: HostNode, parentPath: string): Layer[] {
  return node.children.flatMap((child, index) => walkNode(child, `${parentPath}.${index}`));
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
      const layers = walkChildren(instance, 'root');
      return {
        width: config.width,
        height: config.height,
        frameRate: config.frameRate,
        time,
        layers,
      };
    },
  };
}
