// `@celesta/react/jsx-dev-runtime`: the editor preview routes esbuild's
// `jsxDEV` calls here. Elements are the ones `react/jsx-runtime` builds; the
// position each was written at is kept on the side, keyed by the element's
// props, so a component never sees an extra prop. Fibers carry those props, so
// `sourceOfProps` answers where a rendered element came from.

import { Fragment, jsx, jsxs } from 'react/jsx-runtime';
import type { Key, ReactElement } from 'react';

export interface SourcePosition {
  fileName: string;
  lineNumber: number;
  columnNumber: number;
}

const positions = new WeakMap<object, SourcePosition>();

export { Fragment };

export function jsxDEV(
  type: unknown,
  props: Record<string, unknown>,
  key: Key | undefined,
  isStaticChildren: boolean,
  source?: SourcePosition,
  _self?: unknown,
): ReactElement {
  const element = (isStaticChildren ? jsxs : jsx)(type as never, props, key);
  if (source) positions.set(element.props as object, source);
  return element;
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
