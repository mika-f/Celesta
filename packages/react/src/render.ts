// Evaluates a Mikan React composition into the same `Scene` JSON shape that
// `mikan_composition::Scene` deserializes on the Rust side. There is no
// react-reconciler here: this is a plain synchronous tree walker over the
// element graph produced by calling component functions directly. It
// composes ordinary function components (props in, JSX out) but does not
// implement React's hook dispatcher, so hooks such as `useState` are not
// supported yet.

import * as React from 'react';

import { Composition, Group, Image, Text } from './components';
import type {
  CompositionConfig,
  EvaluatedTransform,
  Layer,
  LayerContent,
  Point,
  ResolvedAsset,
  Scene,
  TextStyle,
  Time,
} from './scene';

type ComponentMarker = typeof Group | typeof Image | typeof Text;

const MARKERS: ReadonlySet<ComponentMarker> = new Set([Group, Image, Text]);
const ELEMENT_TYPE = Symbol.for('react.element');
const MAX_UNWRAP_DEPTH = 1000;

type AnyProps = Record<string, unknown>;
type AnyElement = React.ReactElement<AnyProps, React.ElementType>;
export type EntryComponent = (props: AnyProps) => React.ReactNode;

function isElement(node: unknown): node is AnyElement {
  return (
    node !== null &&
    typeof node === 'object' &&
    (node as { $$typeof?: symbol }).$$typeof === ELEMENT_TYPE
  );
}

function toChildArray(children: unknown): unknown[] {
  if (children === undefined || children === null || typeof children === 'boolean') {
    return [];
  }
  return Array.isArray(children) ? children : [children];
}

function unwrapToComposition(node: unknown): AnyElement {
  let current = node;
  for (let depth = 0; depth < MAX_UNWRAP_DEPTH; depth += 1) {
    if (!isElement(current)) {
      throw new Error("the entry module's default export must render a <Composition> element");
    }
    if (current.type === Composition) {
      return current;
    }
    if (typeof current.type !== 'function') {
      throw new Error(
        `unsupported element <${describeType(current.type)}>; use Mikan's built-in components or a component function`,
      );
    }
    current = (current.type as EntryComponent)(current.props);
  }
  throw new Error('composition root nesting is too deep; check for a component that returns itself');
}

function describeType(type: unknown): string {
  if (typeof type === 'string') {
    return type;
  }
  if (typeof type === 'function') {
    return (type as { name?: string }).name || 'anonymous component';
  }
  return String(type);
}

function numberOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function extractTransform(props: AnyProps): EvaluatedTransform {
  const scale = numberOr(props.scale, 1);
  const position: Point = { x: numberOr(props.x, 0), y: numberOr(props.y, 0) };
  const anchor: Point = { x: numberOr(props.anchorX, 0.5), y: numberOr(props.anchorY, 0.5) };
  return {
    position,
    scale: { x: numberOr(props.scaleX, scale), y: numberOr(props.scaleY, scale) },
    rotation: numberOr(props.rotation, 0),
    anchor,
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

function buildLayer(type: ComponentMarker, props: AnyProps, path: string): Layer {
  const id = typeof props.id === 'string' && props.id.length > 0 ? props.id : path;
  const transform = extractTransform(props);
  const opacity = numberOr(props.opacity, 1);

  let content: LayerContent;
  if (type === Group) {
    content = { type: 'group', layers: renderChildren(props.children, path) };
  } else if (type === Image) {
    content = { type: 'image', asset: resolveAsset(props.src) };
  } else if (type === Text) {
    const maxWidth = props.maxWidth;
    content = {
      type: 'text',
      text: extractText(props.children),
      style: (props.style as TextStyle | undefined) ?? {},
      ...(typeof maxWidth === 'number' ? { maxWidth } : {}),
    };
  } else {
    throw new Error('unreachable: unknown Mikan component marker');
  }

  return { id, transform, opacity, content };
}

function renderNode(node: unknown, path: string): Layer[] {
  if (node === null || node === undefined || typeof node === 'boolean') {
    return [];
  }
  if (Array.isArray(node)) {
    return node.flatMap((child, index) => renderNode(child, `${path}.${index}`));
  }
  if (!isElement(node)) {
    throw new Error(
      'only elements created from Mikan components are supported inside a composition; found a bare string, number, or other value',
    );
  }
  const { type, props } = node;
  if (type === Composition) {
    throw new Error('<Composition> may only appear once, as the single root element');
  }
  if (MARKERS.has(type as ComponentMarker)) {
    return [buildLayer(type as ComponentMarker, props, path)];
  }
  if (typeof type !== 'function') {
    throw new Error(
      `unsupported element <${describeType(type)}>; use Mikan's built-in components or a component function`,
    );
  }
  return renderNode((type as EntryComponent)(props), path);
}

function renderChildren(children: unknown, parentPath: string): Layer[] {
  return toChildArray(children).flatMap((child, index) => renderNode(child, `${parentPath}.${index}`));
}

export function readConfig(defaultExport: EntryComponent): CompositionConfig {
  const composition = unwrapToComposition(React.createElement(defaultExport, {}));
  const { width, height, fps, durationInFrames } = composition.props;
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

export function renderSceneAt(defaultExport: EntryComponent, time: Time): Scene {
  const composition = unwrapToComposition(React.createElement(defaultExport, {}));
  const { width, height, fps } = composition.props;
  const layers = renderChildren(composition.props.children, 'root');
  return {
    width: width as number,
    height: height as number,
    frameRate: { numerator: fps as number, denominator: 1 },
    time,
    layers,
  };
}
