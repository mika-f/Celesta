// Browser entry for the same React host and scene evaluator used by the CLI.
// File-system helpers and project loading are deliberately outside this entry.
import * as React from 'react';

export { React };
export { mount } from './render';
export type { MountedComposition, EntryComponent } from './render';
export {
  Audio, Assets, Character, CharacterView, Composition, Dialogue, Font,
  FreezeFrame, Group, Image, Rect, Sequence, Text, Video,
} from './components';
export { useCurrentFrame, useCurrentTime, useIsPreview, useVideoConfig } from './hooks';
export { Easings, interpolate, interpolateColor, progress, spring } from './animation';
export { frameToTimecode, timecodeToFrame } from './time';
export { beatAt, cueAt, useBeat, useCue } from './timing';
export { Series, Stagger, computeSeries } from './series';
export { TransitionSeries, computeTransitionSeries, useTransitionSeriesScene, useTransitionVolume } from './transition-series';
export { Arrow, Circle, Ellipse, Line, Path, Polyline, pointOnPolyline } from './shapes';
export { Camera } from './camera';
export { TextReveal, useCountUp, useTypewriter } from './text-motion';
export { frameKeyframes } from './keyframes';
export { measureText, textCaret, useTextMetrics } from './text-measure';
export { Span } from './rich-text';
export { TextBox, fitText, useFitText } from './text-fit';
export { Transition } from './transition';
export { Center, Fit, Grid, SafeArea, Stack, useLayoutBounds } from './layout';
export { DebugBounds, DebugOverlay } from './debug';
export { registerComponent, getComponentSchema } from './registry';
export { defineProjectProperties, getProjectProperty, listProjectProperties } from './properties';
export { mediaDurationInFrames } from './media';
