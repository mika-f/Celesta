import * as React from 'react';
import type { ReactNode } from 'react';

import { Sequence } from './components';
import type { CommonProps } from './components';
import { copySourcePosition } from './jsx-dev-runtime';
import { markTransparent } from './source-chain';

export interface SeriesItem {
  /** Length in frames. */
  durationInFrames: number;
  /**
   * Frames to shift this item from the end of the previous one. Negative
   * values overlap it (for a cross-fade), positive values leave a gap.
   * Defaults to 0.
   */
  offset?: number;
}

export interface SeriesTiming {
  /** Where each item starts and how long it lasts, in the series' frames. */
  sequences: { from: number; durationInFrames: number }[];
  /** The frame the last item ends on: the series' total length. */
  durationInFrames: number;
}

/**
 * Lays items out back to back, the way `<Series>` does, without rendering
 * anything. Use it to size the `<Composition>` from its scenes, or to find
 * where a scene starts for a HUD or a transition placed outside the series.
 */
export function computeSeries(items: readonly SeriesItem[]): SeriesTiming {
  let at = 0;
  let end = 0;
  const sequences = items.map(({ durationInFrames, offset = 0 }, index) => {
    if (!Number.isInteger(durationInFrames) || durationInFrames <= 0) {
      throw new Error(`series item ${index} requires a positive integer durationInFrames`);
    }
    if (!Number.isInteger(offset)) {
      throw new Error(`series item ${index} requires an integer offset`);
    }
    at += offset;
    if (at < 0) {
      throw new Error(`series item ${index} would start before frame 0; reduce its negative offset`);
    }
    const from = at;
    at += durationInFrames;
    end = Math.max(end, at);
    return { from, durationInFrames };
  });
  return { sequences, durationInFrames: end };
}

export interface SeriesSequenceProps extends CommonProps, SeriesItem {
  children?: ReactNode;
}

/** One item of a `<Series>`. Only valid as a direct child of `<Series>`. */
function SeriesSequence(_props: SeriesSequenceProps): never {
  throw new Error('<Series.Sequence> must be a direct child of <Series>');
}

export interface SeriesProps {
  children?: ReactNode;
}

function SeriesRoot({ children }: SeriesProps): ReturnType<typeof React.createElement> {
  const items: React.ReactElement<SeriesSequenceProps>[] = [];
  React.Children.forEach(children, (child) => {
    if (child === null || child === undefined || typeof child === 'boolean') {
      return;
    }
    if (!React.isValidElement(child) || child.type !== SeriesSequence) {
      throw new Error('<Series> children must be <Series.Sequence> elements');
    }
    items.push(child as React.ReactElement<SeriesSequenceProps>);
  });
  const { sequences } = computeSeries(items.map((item) => item.props));
  return React.createElement(
    React.Fragment,
    null,
    items.map((item, index) => {
      const { durationInFrames: _duration, offset: _offset, children: inner, ...props } = item.props;
      const sequence = React.createElement(
        Sequence,
        { key: item.key ?? index, ...props, ...sequences[index] },
        inner,
      );
      // `createElement` copies the config into a props object of its own.
      copySourcePosition(item.props, sequence.props);
      return sequence;
    }),
  );
}

/**
 * Plays its `<Series.Sequence>` children one after another, so scenes are
 * written by length instead of by start frame. Each child is an ordinary
 * `<Sequence>`: frames and media restart at 0 inside it. A negative `offset`
 * overlaps an item with the one before it.
 *
 * ```tsx
 * <Series>
 *   <Series.Sequence durationInFrames={90}><Intro /></Series.Sequence>
 *   <Series.Sequence durationInFrames={240} offset={-10}><Body /></Series.Sequence>
 * </Series>
 * ```
 */
export const Series = Object.assign(SeriesRoot, { Sequence: SeriesSequence });

export interface StaggerProps {
  /** Frames between one child's start and the next. */
  each: number;
  /** Frame the first child starts on. Defaults to 0. */
  from?: number;
  /** Optional length of every child's window; defaults to the rest of the enclosing one. */
  durationInFrames?: number;
  children?: ReactNode;
}

/**
 * Starts each child `each` frames after the previous one by wrapping it in a
 * `<Sequence>`: inside, `useCurrentFrame()` counts from that child's own
 * start, so one entrance animation written for frame 0 cascades down a list.
 * A child is hidden until its start.
 *
 * ```tsx
 * <Stagger each={4}>
 *   {rows.map((row, i) => <Row key={row.id} y={i * 60} {...row} />)}
 * </Stagger>
 * ```
 */
export function Stagger({ each, from = 0, durationInFrames, children }: StaggerProps): ReturnType<typeof React.createElement> {
  if (!Number.isFinite(each) || each < 0) {
    throw new Error('<Stagger> requires a finite non-negative `each` prop');
  }
  if (!Number.isFinite(from)) {
    throw new Error('<Stagger> requires a finite `from` prop');
  }
  return React.createElement(
    React.Fragment,
    null,
    React.Children.toArray(children).map((child, index) =>
      React.createElement(
        Sequence,
        {
          key: React.isValidElement(child) && child.key !== null ? child.key : index,
          from: from + index * each,
          durationInFrames,
        },
        child,
      ),
    ),
  );
}

// Both draw their caller's children, each in a `<Sequence>` of their own.
for (const wrapper of [SeriesRoot, Stagger]) markTransparent(wrapper);
