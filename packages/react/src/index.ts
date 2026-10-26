/// <reference path="../subtitle-files.d.ts" preserve="true" />

export {
  Audio,
  Assets,
  Composition,
  FreezeFrame,
  Group,
  Image,
  Instances,
  Loop,
  Mask,
  Rect,
  Sequence,
  Static,
  Text,
  Video,
  Font,
} from './components';
export type {
  AudioDuck,
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
  LoopProps,
  MaskProps,
  RectProps,
  SequenceProps,
  StaticProps,
  TextProps,
  VideoProps,
} from './components';
export { MotionBlur, Trail } from './motion-blur';
export type { MomentChildren, MotionBlurProps, TrailProps } from './motion-blur';
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
  FontFallbackMode,
  FontFamily,
  FontFeatures,
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
  TextStroke,
  TextStyle,
  Time,
} from './scene';
export type {
  BuiltinFilterName, BuiltinFilterValues, ColorFilterName, ColorFilterValues, EffectFilterName, EffectFilterValues,
  LayerChromaKey, LayerChromaticAberration, LayerHalftone, LayerNoise, LayerPixelate, LayerScanlines, LayerTint,
  LayerVignette, LayerWave, LayerZoomBlur, ShaderEffect,
} from './shader';

export { useCurrentFrame, useCurrentTime, useDebugGuides, useIsPreview, useVideoConfig } from './hooks';
export type { VideoConfig } from './hooks';

export { frameToTimecode, timecodeToFrame } from './time';
export type { TimecodeOptions } from './time';

export { beatAt, cueAt, useBeat, useCue } from './timing';
export type { ActiveCue, Beat, BeatOptions, Cue } from './timing';

export { Series, Stagger, computeSeries } from './series';
export type { SeriesItem, SeriesProps, SeriesSequenceProps, SeriesTiming, StaggerProps } from './series';

export { Scenes, defineScenes, sceneAt, useScene } from './scenes';
export type { PlacedScene, SceneEntry, SceneList, SceneState, ScenesProps, UseSceneOptions } from './scenes';

export { listChapters } from './chapters';
export type { Chapter, ChapterInput, ChapterSource, ListChaptersOptions } from './chapters';

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
export { Theme, useTheme } from './theme';
export type { ResolvedTheme, TextShorthand, ThemeProps, ThemeValues } from './theme';

export { Subtitles, formatSubtitles, parseSubtitles } from './subtitles';
export type {
  FormatSubtitlesOptions,
  ParseSubtitlesOptions,
  SubtitleCue,
  SubtitleFormat,
  SubtitlesProps,
} from './subtitles';

export { getComponentSchema, registerComponent } from './registry';
export type { ComponentDefinition, ComponentPropertyField, ComponentPropertySchema, ComponentSchemaField } from './registry';

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
