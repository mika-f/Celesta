/// <reference path="../subtitle-files.d.ts" preserve="true" />

export {
  Audio,
  Assets,
  Composition,
  FreezeFrame,
  Group,
  Image,
  Instances,
  Mask,
  Rect,
  Sequence,
  Static,
  Text,
  Video,
  Font,
} from './components';
export type {
  AudioProps,
  AssetInput,
  AssetReference,
  AssetsProps,
  FontProps,
  FreezeFrameProps,
  AnimatedNumber,
  ClipRect,
  CommonProps,
  CompositionProps,
  GroupProps,
  ImageProps,
  InstanceColumn,
  InstancesProps,
  MaskProps,
  RectProps,
  SequenceProps,
  StaticProps,
  TextProps,
  VideoProps,
} from './components';
export { googleFont } from './google-font';
export type { GoogleFontOptions } from './google-font';
export type {
  AssetLocation,
  AudioClip,
  AudioGraph,
  BlendMode,
  Clip,
  CompositionConfig,
  CompositionInfo,
  EvaluatedTransform,
  FontFamily,
  GradientStop,
  GroupMask,
  Layer,
  LayerContent,
  LayerEffects,
  LayerGlow,
  LayerFilter,
  LayerShader,
  LayerShadow,
  LetterSpacing,
  LineBreak,
  LineCap,
  LineDash,
  LineJoin,
  MaskMode,
  MediaTiming,
  Paint,
  PathCommand,
  Point,
  Rational,
  ResolvedAsset,
  Scene,
  ShaderParam,
  ShaderParamType,
  ShaderSource,
  Stroke,
  TextAlign,
  TextColorRun,
  TextFontRun,
  TextStyle,
  Time,
} from './scene';
export type { ColorFilterName, ColorFilterValues, LayerTint, LayerVignette, ShaderEffect } from './shader';

export { useCurrentFrame, useCurrentTime, useDebugGuides, useIsPreview, useVideoConfig } from './hooks';
export type { VideoConfig } from './hooks';

export { frameToTimecode, timecodeToFrame } from './time';

export { beatAt, cueAt, useBeat, useCue } from './timing';
export type { ActiveCue, Beat, BeatOptions, Cue } from './timing';

export { Series, Stagger, computeSeries } from './series';
export type { SeriesItem, SeriesProps, SeriesSequenceProps, SeriesTiming, StaggerProps } from './series';

export { Easings, interpolate, interpolateColor, progress, spring } from './animation';
export type {
  ColorExtrapolate,
  Extrapolate,
  InterpolateColorOptions,
  InterpolateOptions,
  SpringConfig,
  SpringOptions,
} from './animation';

export { frameKeyframes } from './keyframes';
export type { FrameKeyframe, FrameKeyframesOptions } from './keyframes';

export { measureText, textCaret, useTextMetrics } from './text-measure';
export type { GlyphMetrics, MeasureTextOptions, TextMetrics } from './text-measure';

export { Span } from './rich-text';
export type { SpanProps, SpanStyle } from './rich-text';

export { Subtitles, formatSubtitles, parseSubtitles } from './subtitles';
export type {
  FormatSubtitlesOptions,
  ParseSubtitlesOptions,
  SubtitleCue,
  SubtitleFormat,
  SubtitlesProps,
} from './subtitles';

export { getComponentSchema, registerComponent } from './registry';
export type { ComponentDefinition, ComponentPropertyField, ComponentPropertySchema } from './registry';

export { defineProjectProperties, getProjectProperty, listProjectProperties } from './properties';
export type { ProjectPropertyField, ProjectPropertySchema } from './properties';

export type { Animatable } from './generated/Animatable';
export type { AnimatablePoint } from './generated/AnimatablePoint';
export type { Easing } from './generated/Easing';
export type { Keyframe } from './generated/Keyframe';
export type { KeyframeAnimation } from './generated/KeyframeAnimation';
export type { KeyframeAnimationType } from './generated/KeyframeAnimationType';
export type { TimeRange } from './generated/TimeRange';
export type { Transform } from './generated/Transform';
export type { JsonValue } from './generated/serde_json/JsonValue';
