// `@celesta/react/jsx-dev-runtime`: the editor preview routes esbuild's
// `jsxDEV` calls here. Elements are the ones `react/jsx-runtime` builds; the
// position each was written at is kept on the side, keyed by the element's
// props, so a component never sees an extra prop. Fibers carry those props, so
// `sourceOfProps` answers where a rendered element came from.

import { createContext, createElement, use } from 'react';
import { Fragment, jsx, jsxs } from 'react/jsx-runtime';
import type { Key, ReactElement } from 'react';

export interface SourcePosition {
  fileName: string;
  lineNumber: number;
  columnNumber: number;
}

/** Where an element was written, as a layer source chain reports it. */
export interface OverridePosition {
  file: string;
  line: number;
  column: number;
}

/**
 * A value the editor's drag preview gives the prop `prop` of every element
 * written at `file:line:column` (an absolute path; 1-based, as reported in a
 * layer source chain) for one frame, in place of the written one. `prop` is a
 * prop name, or `style.<key>` for one key of the `style` prop. With `within`,
 * only the elements created under a component element written there (one
 * call of the component they are written in) take it; the copies under its
 * other calls keep their values.
 */
export interface PropOverride extends OverridePosition {
  prop: string;
  value: number;
  within?: OverridePosition;
}

const positions = new WeakMap<object, SourcePosition>();
/** The props of the elements created at an override's position this frame. */
let atOverride = new WeakSet<object>();

/** The overrides of the frame being rendered, by `line:column`; `null` between frames and without any. */
let overrides: Map<string, PropOverride[]> | null = null;
/** The files of the overrides' `within` positions, by `line:column`; empty without any. */
let withinFiles = new Map<string, Set<string>>();
let resolveFile: (fileName: string) => string = (fileName) => fileName;
/** `resolveFile`'s answers this frame. */
const resolvedFiles = new Map<string, string>();

/**
 * Provided above the entry with a new value whenever the overrides change: a
 * component that created an element at an override's position renders again
 * then, even a `memo` one whose props did not change.
 */
export const PropOverridesContext = createContext<object | null>(null);

/**
 * The `within` positions (`file:line:column`) of the component elements the
 * one rendering was created under, outermost first.
 */
const WithinContext = createContext<readonly string[]>([]);
/** Each `WithinContext` value with one more position, so a value stays the same object across frames. */
const extended = new WeakMap<readonly string[], Map<string, readonly string[]>>();
/** The component each `within` boundary renders, by that component. */
const boundaries = new WeakMap<object, (props: Record<string, unknown>) => ReactElement>();

/**
 * A component rendering `type` with its props under `WithinContext`
 * extended by the position its element was created at: the element type
 * `jsxDEV` gives an element at a `within` position, so the elements created
 * below it can tell this call from the component's other calls.
 */
function boundary(type: object): (props: Record<string, unknown>) => ReactElement {
  let wrapper = boundaries.get(type);
  if (wrapper) return wrapper;
  wrapper = (props: Record<string, unknown>) => {
    const outer = use(WithinContext);
    const position = positionKey(sourceOfProps(props));
    let next = outer;
    if (position !== undefined && !outer.includes(position)) {
      let byPosition = extended.get(outer);
      if (!byPosition) extended.set(outer, (byPosition = new Map()));
      next = byPosition.get(position) ?? [...outer, position];
      byPosition.set(position, next);
    }
    // `jsx` keeps `props` as the element's props (they hold no `key`), so
    // the inner element has the recorded position too.
    return createElement(WithinContext.Provider, { value: next }, jsx(type as never, props));
  };
  const named = type as { displayName?: string; name?: string };
  Object.defineProperty(wrapper, 'name', { value: named.displayName || named.name || 'Anonymous' });
  boundaries.set(type, wrapper);
  return wrapper;
}

/** `source` as `WithinContext` and `withinFiles` name it, its file resolved. */
function positionKey(source: SourcePosition | undefined): string | undefined {
  return source && `${resolved(source.fileName)}:${source.lineNumber}:${source.columnNumber}`;
}

function resolved(fileName: string): string {
  let file = resolvedFiles.get(fileName);
  if (file === undefined) {
    file = resolveFile(fileName);
    resolvedFiles.set(fileName, file);
  }
  return file;
}

/** Whether `type`, created at `source`, is a component element at an override's `within` position. */
function atWithin(type: unknown, source: SourcePosition): type is object {
  if (typeof type !== 'function' && (typeof type !== 'object' || type === null)) return false;
  if (type === Fragment) return false;
  const files = withinFiles.get(`${source.lineNumber}:${source.columnNumber}`);
  return files !== undefined && files.has(resolved(source.fileName));
}

export { Fragment };

export function jsxDEV(
  type: unknown,
  props: Record<string, unknown>,
  key: Key | undefined,
  isStaticChildren: boolean,
  source?: SourcePosition,
  _self?: unknown,
): ReactElement {
  const atPosition = overrides !== null && source ? overridden(props, source) : undefined;
  const created = overrides !== null && source && atWithin(type, source) ? boundary(type) : type;
  const element = (isStaticChildren ? jsxs : jsx)(created as never, atPosition ?? props, key);
  if (source) positions.set(element.props as object, source);
  if (atPosition) atOverride.add(element.props as object);
  return element;
}

/**
 * Whether `children` (elements, nested in arrays and in their own
 * `children`) hold one created at an override's position this frame.
 */
export function createdAtOverride(children: unknown): boolean {
  if (overrides === null) return false;
  if (Array.isArray(children)) return children.some(createdAtOverride);
  if (typeof children !== 'object' || children === null || !('props' in children)) return false;
  const { props } = children as { props: unknown };
  return typeof props === 'object' && props !== null &&
    (atOverride.has(props) || createdAtOverride((props as { children?: unknown }).children));
}

/** An override's `prop` prefix naming a key inside the `style` prop. */
const STYLE_KEY = 'style.';

/**
 * `props` with the overrides for `source`, or `undefined` when `source` is
 * no override's position. An override with `within` applies only under a
 * boundary created at that position (see `boundary`), which `WithinContext`
 * tells. A `style.<key>` override sets that key in a copy
 * of `style` (an object holding only it when `style` is not written). Only a
 * component creating an element at an override's position subscribes to
 * later ones (`use` reads a context without a hook slot, so it may be called
 * conditionally); every other component keeps React's bailouts on drag
 * frames.
 */
function overridden(props: Record<string, unknown>, source: SourcePosition): Record<string, unknown> | undefined {
  const matches = overrides?.get(`${source.lineNumber}:${source.columnNumber}`);
  if (!matches) return undefined;
  const file = resolved(source.fileName);
  let result = props;
  /** This call's copy of `style`, once a `style.<key>` override needs one. */
  let style: Record<string, unknown> | undefined;
  let subscribed = false;
  /** The `within` positions above, once an override names one. */
  let within: readonly string[] | undefined;
  for (const { file: target, prop, value, within: boundaryAt } of matches) {
    if (target !== file) continue;
    if (!subscribed) {
      subscribed = true;
      try {
        use(PropOverridesContext);
      } catch {
        // Not called while a component renders (an effect, say): nothing to subscribe.
      }
    }
    if (boundaryAt) {
      if (within === undefined) {
        try {
          within = use(WithinContext);
        } catch {
          within = [];
        }
      }
      if (!within.includes(`${boundaryAt.file}:${boundaryAt.line}:${boundaryAt.column}`)) continue;
    }
    if (prop.startsWith(STYLE_KEY)) {
      const key = prop.slice(STYLE_KEY.length);
      const current = result.style as Record<string, unknown> | null | undefined;
      if (Object.is(current?.[key], value)) continue;
      if (style === undefined) {
        style = { ...current };
        if (result === props) result = { ...props };
        result.style = style;
      }
      style[key] = value;
    } else if (!Object.is(result[prop], value)) {
      if (result === props) result = { ...props };
      result[prop] = value;
    }
  }
  return subscribed ? result : undefined;
}

/**
 * Sets the overrides elements are created with until `clearPropOverrides`.
 * `resolve` turns a recorded file name into the absolute path overrides name.
 */
export function setPropOverrides(list: readonly PropOverride[], resolve: (fileName: string) => string): void {
  resolvedFiles.clear();
  resolveFile = resolve;
  overrides = new Map();
  withinFiles = new Map();
  for (const entry of list) {
    const position = `${entry.line}:${entry.column}`;
    const at = overrides.get(position);
    if (at) at.push(entry);
    else overrides.set(position, [entry]);
    if (entry.within) {
      const boundaryAt = `${entry.within.line}:${entry.within.column}`;
      const files = withinFiles.get(boundaryAt);
      if (files) files.add(entry.within.file);
      else withinFiles.set(boundaryAt, new Set([entry.within.file]));
    }
  }
}

export function clearPropOverrides(): void {
  overrides = null;
  withinFiles = new Map();
  atOverride = new WeakSet();
}

export function sourceOfProps(props: unknown): SourcePosition | undefined {
  return typeof props === 'object' && props !== null ? positions.get(props) : undefined;
}

/**
 * Gives the props object `to` the position recorded for `from`, if any: for a
 * component that renders the element it was handed as another one
 * (`<Series.Sequence>` as a `<Sequence>`), so that one points at the JSX.
 */
export function copySourcePosition(from: object, to: object): void {
  const source = positions.get(from);
  if (source) positions.set(to, source);
}
