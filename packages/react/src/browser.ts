// Browser entry for the same React host and scene evaluator used by the CLI.
// File-system helpers are deliberately outside this entry.
import * as React from 'react';

export { React };
export {
  Audio, Assets, Composition, Font, FreezeFrame, Group, Image, Rect, Sequence, Text, Video,
} from './components';
export { useCurrentFrame, useCurrentTime, useIsPreview, useVideoConfig } from './hooks';
export { Easings, interpolate, interpolateColor, progress, spring } from './animation';
export { frameToTimecode, timecodeToFrame } from './time';
export { beatAt, cueAt, useBeat, useCue } from './timing';
export { Series, Stagger, computeSeries } from './series';
export { frameKeyframes } from './keyframes';
export { measureText, textCaret, useTextMetrics } from './text-measure';
export { Span } from './rich-text';
export { registerComponent, getComponentSchema } from './registry';
export { defineProjectProperties, getProjectProperty, listProjectProperties } from './properties';
