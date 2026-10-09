import * as React from 'react';

import { Easings } from './animation';
import { Text } from './components';
import type { GroupProps, TextProps } from './components';
import { useVideoConfig } from './hooks';
import { useLayoutBounds } from './layout';
import type { TextStyle } from './scene';
import { Transition } from './transition';
import type { SlideFrom, TransitionType } from './transition';

export interface TypographyProps {
  font?: string;
  size?: number;
  weight?: number;
  color?: string;
  align?: TextStyle['align'];
  /** Full text style; explicit fields override the shorthand props. */
  style?: TextStyle;
}

export interface ThemeProps extends TypographyProps {
  children?: React.ReactNode;
}

const ThemeContext = React.createContext<TextStyle>({});

function typographyStyle({ font, size, weight, color, align, style }: TypographyProps): TextStyle {
  return {
    ...(font === undefined ? {} : { fontFamily: font }),
    ...(size === undefined ? {} : { fontSize: size }),
    ...(weight === undefined ? {} : { fontWeight: weight }),
    ...(color === undefined ? {} : { fill: { type: 'solid' as const, color } }),
    ...(align === undefined ? {} : { align }),
    ...style,
  };
}

/** Shares text defaults with descendant Titles. Nested themes merge their styles. */
export function Theme({ children, ...props }: ThemeProps): ReturnType<typeof React.createElement> {
  const inherited = React.useContext(ThemeContext);
  return React.createElement(ThemeContext.Provider, { value: { ...inherited, ...typographyStyle(props) } }, children);
}

export type MotionPreset = 'fade' | 'slide-up' | 'slide-down' | 'slide-left' | 'slide-right' | 'scale-in';

export interface MotionOptions {
  type: MotionPreset;
  /** Defaults to half a second, rounded to at least one frame. */
  durationInFrames?: number;
  /** Slide distance in pixels; defaults to 64. */
  distance?: number;
  /** Starting scale for scale-in; defaults to 0.8. */
  scaleFrom?: number;
  /** Defaults to Easings.easeOutCubic. */
  easing?: (progress: number) => number;
}

export type MotionEntrance = MotionPreset | MotionOptions;

export interface MotionProps extends GroupProps {
  enter: MotionEntrance;
}

const MOTIONS: Record<MotionPreset, { type: readonly TransitionType[]; slideFrom?: SlideFrom }> = {
  fade: { type: ['fade'] },
  'slide-up': { type: ['fade', 'slide'], slideFrom: 'bottom' },
  'slide-down': { type: ['fade', 'slide'], slideFrom: 'top' },
  'slide-left': { type: ['fade', 'slide'], slideFrom: 'right' },
  'slide-right': { type: ['fade', 'slide'], slideFrom: 'left' },
  'scale-in': { type: ['fade', 'scale'] },
};

/** A preset entrance on the enclosing sequence's clock, using the shared Transition evaluator. */
export function Motion({ enter, children, ...props }: MotionProps): ReturnType<typeof Transition> {
  const { fps } = useVideoConfig();
  const options = typeof enter === 'string' ? { type: enter } : enter;
  if (!Object.prototype.hasOwnProperty.call(MOTIONS, options.type)) {
    throw new Error(`unknown <Motion> entrance ${JSON.stringify(options.type)}`);
  }
  const preset = MOTIONS[options.type];
  if (options.distance !== undefined && (!Number.isFinite(options.distance) || options.distance < 0)) {
    throw new Error('<Motion> distance must be a finite non-negative number');
  }
  if (options.scaleFrom !== undefined && (!Number.isFinite(options.scaleFrom) || options.scaleFrom < 0)) {
    throw new Error('<Motion> scaleFrom must be a finite non-negative number');
  }
  return React.createElement(Transition, {
    ...props,
    ...preset,
    durationInFrames: options.durationInFrames ?? Math.max(1, Math.round(fps / 2)),
    distance: options.distance,
    scaleFrom: options.scaleFrom,
    easing: options.easing ?? Easings.easeOutCubic,
    children,
  });
}

export interface TitleProps extends Omit<TextProps, 'style'>, TypographyProps {
  /** Centers in the current layout bounds. Explicit coordinates and anchors take priority. */
  center?: boolean;
  enter?: MotionEntrance;
}

/** Text with typography shortcuts, optional centering, and an optional entrance. */
export function Title({
  font, size, weight, color, align, style, center = false, enter,
  x, y, anchorX, anchorY, ...props
}: TitleProps): ReturnType<typeof React.createElement> {
  const theme = React.useContext(ThemeContext);
  const bounds = useLayoutBounds();
  const positionX = x ?? (center ? bounds.width / 2 : 0);
  const positionY = y ?? (center ? bounds.height / 2 : 0);
  const text = React.createElement(Text, {
    ...props,
    x: enter === undefined ? positionX : 0,
    y: enter === undefined ? positionY : 0,
    anchorX: anchorX ?? (center ? 0.5 : 0),
    anchorY: anchorY ?? (center ? 0.5 : 0),
    style: {
      fontFamily: 'sans-serif',
      fontSize: 96,
      fill: { type: 'solid', color: '#ffffff' },
      ...theme,
      ...(center ? { align: 'center' as const } : {}),
      ...typographyStyle({ font, size, weight, color, align, style }),
    },
  });
  return enter === undefined ? text : React.createElement(Motion, { enter, x: positionX, y: positionY }, text);
}
