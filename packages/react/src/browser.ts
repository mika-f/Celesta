// Browser entry for the same React host and scene evaluator used by the CLI.
// File-system helpers are deliberately outside this entry.
import * as React from 'react';

export { React };
export { googleFont } from './google-font';
export {
  Audio, Assets, Composition, Font, FreezeFrame, Group, Image, Instances, Loop, Mask, Rect, Sequence, Static, Text, Video,
} from './components';
export { MotionBlur, Trail } from './motion-blur';
export { useCurrentFrame, useCurrentTime, useDebugGuides, useIsPreview, useVideoConfig } from './hooks';
export { Easings, interpolate, interpolateColor, progress, spring } from './animation';
export { frameToTimecode, timecodeToFrame } from './time';
export { beatAt, cueAt, useBeat, useCue } from './timing';
export { Series, Stagger, computeSeries } from './series';
export { Scenes, defineScenes, sceneAt, useScene } from './scenes';
export { ScenePlaceholder } from './scene-placeholder';
export { listChapters } from './chapters';
export { frameKeyframes, keyframes } from './keyframes';
export { measureText, textCaret, useTextMetrics } from './text-measure';
export { Span } from './rich-text';
export { Theme, useTheme } from './theme';
export { Subtitles, formatSubtitles, parseSubtitles } from './subtitles';
export { registerComponent, getComponentSchema } from './registry';
export { defineProjectProperties, getProjectProperty, listProjectProperties } from './properties';
