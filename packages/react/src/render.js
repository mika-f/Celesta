'use strict';

// Evaluates a Mikan React composition into the same `Scene` JSON shape that
// `mikan_composition::Scene` deserializes on the Rust side. There is no
// react-reconciler here: this is a plain synchronous tree walker over the
// element graph produced by calling component functions directly. It
// composes ordinary function components (props in, JSX out) but does not
// implement React's hook dispatcher, so hooks such as `useState` are not
// supported yet.

const React = require('react');
const { Composition, Group, Image, Text } = require('./components');

const MARKERS = new Set([Group, Image, Text]);
const ELEMENT_TYPE = Symbol.for('react.element');
const MAX_UNWRAP_DEPTH = 1000;

function isElement(node) {
  return node !== null && typeof node === 'object' && node.$$typeof === ELEMENT_TYPE;
}

function toChildArray(children) {
  if (children === undefined || children === null || typeof children === 'boolean') {
    return [];
  }
  return Array.isArray(children) ? children : [children];
}

function unwrapToComposition(node) {
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
    current = current.type(current.props);
  }
  throw new Error('composition root nesting is too deep; check for a component that returns itself');
}

function describeType(type) {
  if (typeof type === 'string') {
    return type;
  }
  if (typeof type === 'function') {
    return type.name || 'anonymous component';
  }
  return String(type);
}

function numberOr(value, fallback) {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function extractTransform(props) {
  const scale = numberOr(props.scale, 1);
  return {
    position: { x: numberOr(props.x, 0), y: numberOr(props.y, 0) },
    scale: { x: numberOr(props.scaleX, scale), y: numberOr(props.scaleY, scale) },
    rotation: numberOr(props.rotation, 0),
    anchor: { x: numberOr(props.anchorX, 0.5), y: numberOr(props.anchorY, 0.5) },
  };
}

function extractText(children) {
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

function resolveAsset(src) {
  if (typeof src !== 'string' || src.length === 0) {
    throw new Error('components with asset content require a non-empty `src` prop');
  }
  return { id: src, location: { type: 'file', path: src } };
}

function buildLayer(type, props, path) {
  const id = typeof props.id === 'string' && props.id.length > 0 ? props.id : path;
  const transform = extractTransform(props);
  const opacity = numberOr(props.opacity, 1);

  let content;
  if (type === Group) {
    content = { type: 'group', layers: renderChildren(props.children, path) };
  } else if (type === Image) {
    content = { type: 'image', asset: resolveAsset(props.src) };
  } else if (type === Text) {
    content = {
      type: 'text',
      text: extractText(props.children),
      style: props.style ?? {},
      ...(typeof props.maxWidth === 'number' ? { maxWidth: props.maxWidth } : {}),
    };
  } else {
    throw new Error('unreachable: unknown Mikan component marker');
  }

  return { id, transform, opacity, content };
}

function renderNode(node, path) {
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
  if (MARKERS.has(type)) {
    return [buildLayer(type, props, path)];
  }
  if (typeof type !== 'function') {
    throw new Error(
      `unsupported element <${describeType(type)}>; use Mikan's built-in components or a component function`,
    );
  }
  return renderNode(type(props), path);
}

function renderChildren(children, parentPath) {
  return toChildArray(children).flatMap((child, index) => renderNode(child, `${parentPath}.${index}`));
}

function readConfig(defaultExport) {
  const composition = unwrapToComposition(React.createElement(defaultExport, {}));
  const { width, height, fps, durationInFrames } = composition.props;
  if (!Number.isInteger(width) || width <= 0) {
    throw new Error('<Composition> requires a positive integer `width` prop');
  }
  if (!Number.isInteger(height) || height <= 0) {
    throw new Error('<Composition> requires a positive integer `height` prop');
  }
  if (!Number.isInteger(fps) || fps <= 0) {
    throw new Error('<Composition> requires a positive integer `fps` prop');
  }
  if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
    throw new Error('<Composition> requires a positive integer `durationInFrames` prop');
  }
  return {
    width,
    height,
    frameRate: { numerator: fps, denominator: 1 },
    durationInFrames,
  };
}

function renderSceneAt(defaultExport, time) {
  const composition = unwrapToComposition(React.createElement(defaultExport, {}));
  const { width, height, fps } = composition.props;
  const layers = renderChildren(composition.props.children, 'root');
  return {
    width,
    height,
    frameRate: { numerator: fps, denominator: 1 },
    time,
    layers,
  };
}

module.exports = { readConfig, renderSceneAt };
