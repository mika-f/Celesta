// The prop checks the components share. Every message starts with the
// component's tag and ends with the value it was given, so
// `validators('Panel').nonNegative('width', -1)` throws
// `<Panel> width must be a finite number of 0 or more, not -1`.

import type { Insets } from '@celesta/layout';

/** The checks for one component's props, each naming `component` in its error. */
export interface Validators {
  /** Throws `message`, prefixed with the component's tag. */
  fail(message: string): never;
  /** `value`, checked to be a finite number. */
  finite(name: string, value: number): number;
  /** `value`, checked to be a finite number of 0 or more. */
  nonNegative(name: string, value: number): number;
  /** `value`, checked to be a finite number greater than 0. */
  positive(name: string, value: number): number;
  /** `padding` as pixels on every edge, each checked to be 0 or more; a missing edge is 0. */
  insets(padding: number | Insets): Required<Insets>;
}

/** The prop checks for `<component>`. */
export function validators(component: string): Validators {
  const fail = (message: string): never => {
    throw new Error(`<${component}> ${message}`);
  };
  const finite = (name: string, value: number): number => {
    if (typeof value !== 'number' || !Number.isFinite(value)) fail(`${name} must be a finite number, not ${value}`);
    return value;
  };
  const nonNegative = (name: string, value: number): number => {
    if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
      fail(`${name} must be a finite number of 0 or more, not ${value}`);
    }
    return value;
  };
  const positive = (name: string, value: number): number => {
    if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
      fail(`${name} must be a finite number greater than 0, not ${value}`);
    }
    return value;
  };
  const insets = (padding: number | Insets): Required<Insets> => {
    const edges = typeof padding === 'number' ? { top: padding, right: padding, bottom: padding, left: padding } : padding;
    return {
      top: nonNegative('padding top', edges.top ?? 0),
      right: nonNegative('padding right', edges.right ?? 0),
      bottom: nonNegative('padding bottom', edges.bottom ?? 0),
      left: nonNegative('padding left', edges.left ?? 0),
    };
  };
  return { fail, finite, nonNegative, positive, insets };
}
