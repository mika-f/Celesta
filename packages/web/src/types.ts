import type { AudioDuck } from '@celesta/react';

/** Types returned by the Celesta React evaluator in the browser. */
export interface CompositionConfig {
  width: number;
  height: number;
  frameRate: { numerator: number; denominator: number };
  durationInFrames: number;
  lang?: string;
}

export type Asset = { id: string; location: { type: 'file'; path: string } | { type: 'url'; url: string } };
export type Paint =
  | { type: 'solid'; color: string }
  | { type: 'linear'; start: Point; end: Point; stops: { offset: number; color: string }[] }
  | { type: 'radial'; center: Point; radius: number; stops: { offset: number; color: string }[] };
export type Stroke = { paint: Paint; width: number };
/** A text's outline: one stroke, or several, each `width` from the glyph outline. */
export type TextStroke = Stroke | readonly Stroke[];
export type LineDash = { pattern: number[]; offset?: number };
export type Point = { x: number; y: number };
export type Transform = {
  position: Point;
  scale: Point;
  rotation: number;
  anchor: Point;
  /**
   * The point the layer turns, scales, and skews about, and its
   * perspective's vanishing point, in its own pixels from its anchor; absent is (0, 0).
   */
  origin?: Point;
  /** Degrees, as CSS `skew(x, y)`; absent is 0. */
  skew?: Point;
  /** Degrees, as CSS `rotateX()` and `rotateY()`; absent is 0. */
  rotationX?: number;
  rotationY?: number;
  /** Viewing distance for the layer's own 3D rotation; absent, or not finite and positive, is none. */
  perspective?: number;
  backfaceVisibility?: 'visible' | 'hidden';
};
export type PathCommand =
  | { type: 'moveTo' | 'lineTo'; x: number; y: number }
  | { type: 'quadTo'; x1: number; y1: number; x: number; y: number }
  | { type: 'cubicTo'; x1: number; y1: number; x2: number; y2: number; x: number; y: number }
  | { type: 'close' };
/** A family name, or a font stack tried in order. */
export type FontFamily = string | readonly string[];
/** OpenType features by tag, as CSS `font-feature-settings`: on, off, or an alternate's number. */
export type FontFeatures = { readonly [tag: string]: boolean | number };
export type FontRun = { start: number; end: number; fontWeight?: number | null; fontFamily?: FontFamily | null; fontFeatures?: FontFeatures | null };
export type TextStyle = {
  lang?: string | null;
  /** `'loaded'` appends the scene's loaded font families to `fontFamily`. */
  fontFallback?: 'system' | 'loaded' | null;
  fontFamily?: FontFamily | null;
  fontSize?: number | null;
  fontWeight?: number | null;
  fill?: Paint | null;
  stroke?: TextStroke | null;
  align?: 'left' | 'center' | 'right' | null;
  lineHeight?: number | null;
  letterSpacing?: number | { em: number } | null;
  /** Needs every family it applies to loaded with `<Font>`; a font run's are merged over these. */
  fontFeatures?: FontFeatures | null;
  colorRuns?: { start: number; end: number; color: string }[];
  fontRuns?: FontRun[];
  visibleCharacters?: number | null;
  lineBreak?: 'normal' | 'phrase' | null;
};
export type LayerContent =
  | {
    type: 'group';
    layers: Layer[];
    clip?: { x: number; y: number; width: number; height: number; cornerRadius: number } | null;
    /** Shows the layers only where `mask.layers` draw; see `GroupMask` in `@celesta/react`. */
    mask?: { layers: Layer[]; mode?: 'alpha' | 'luminance'; invert?: boolean } | null;
    motionBlur?: boolean;
  }
  | { type: 'rect'; width: number; height: number; fill?: Paint | null; stroke?: Stroke | null; cornerRadius: number }
  | { type: 'path'; commands: PathCommand[]; fill?: Paint | null; stroke?: Stroke | null; lineCap?: 'butt' | 'round' | 'square'; lineJoin?: 'miter' | 'round' | 'bevel'; miterLimit?: number; lineDash?: LineDash | null }
  | { type: 'text'; text: string; style: TextStyle; maxWidth?: number | null; baselineAnchor?: boolean }
  | { type: 'image'; asset: Asset; width?: number; height?: number; fit?: 'contain' | 'cover'; animation?: { sourceTimeSeconds: number; loop: boolean } }
  | { type: 'video'; asset: Asset; timing: { sourceTimeSeconds: number; loop?: boolean }; width?: number; height?: number }
  | { type: 'psd'; asset: Asset }
  | { type: 'missingComponent'; component: string; props: Record<string, unknown> };
/** A built-in color filter; see `LayerFilter` in `@celesta/react`. */
export type ColorFilter =
  | { type: 'brightness' | 'contrast' | 'saturation' | 'grayscale' | 'invert'; amount: number }
  | { type: 'hue'; degrees: number }
  | { type: 'tint'; color: string; amount: number }
  | { type: 'vignette'; amount: number; radius: number; softness: number };
/** A built-in stylize effect or the chroma key; see `LayerFilter` in `@celesta/react`. */
export type EffectFilter =
  | { type: 'pixelate'; size: number }
  | { type: 'halftone'; size: number; angle: number }
  | { type: 'chromaticAberration'; offset: number; angle: number }
  | { type: 'wave'; amplitude: number; wavelength: number; phase: number; angle: number }
  | { type: 'noise'; amount: number; seed: number; monochrome: boolean }
  | { type: 'scanlines'; amount: number; spacing: number; thickness: number; offset: number }
  | { type: 'zoomBlur'; amount: number; centerX: number; centerY: number }
  | { type: 'chromaKey'; color: string; tolerance: number; softness: number; spill: number };
export type BuiltinFilter = ColorFilter | EffectFilter;
export type Layer = {
  id: string;
  transform: Transform;
  opacity: number;
  effects?: {
    blur?: number;
    shadow?: { color: string; blur: number; offsetX: number; offsetY: number } | null;
    glow?: { color: string; blur: number } | null;
    filters?: (BuiltinFilter | { type: 'shader'; id: string })[];
  };
  blendMode?:
    | 'normal' | 'multiply' | 'screen' | 'overlay' | 'add' | 'difference'
    | 'darken' | 'lighten' | 'colorDodge' | 'colorBurn' | 'hardLight' | 'softLight' | 'exclusion'
    | 'hue' | 'saturation' | 'color' | 'luminosity';
  content: LayerContent;
};
export type Scene = {
  width: number;
  height: number;
  frameRate: CompositionConfig['frameRate'];
  time: { value: number; timescale: number };
  fonts?: Asset[];
  layers: Layer[];
};
/** Kebab-case names of `crates/composition`'s `Easing`. */
export type KeyframeEasing =
  | 'linear' | 'ease-in' | 'ease-out' | 'ease-in-out'
  | 'ease-in-sine' | 'ease-out-sine' | 'ease-in-out-sine'
  | 'ease-in-quad' | 'ease-out-quad' | 'ease-in-out-quad'
  | 'ease-in-cubic' | 'ease-out-cubic' | 'ease-in-out-cubic'
  | 'ease-in-quart' | 'ease-out-quart' | 'ease-in-out-quart'
  | 'ease-in-quint' | 'ease-out-quint' | 'ease-in-out-quint'
  | 'ease-in-expo' | 'ease-out-expo' | 'ease-in-out-expo'
  | 'ease-in-circ' | 'ease-out-circ' | 'ease-in-out-circ'
  | 'ease-in-back' | 'ease-out-back' | 'ease-in-out-back'
  | 'ease-in-elastic' | 'ease-out-elastic' | 'ease-in-out-elastic'
  | 'ease-in-bounce' | 'ease-out-bounce' | 'ease-in-out-bounce';
/** `easing` absent or `null` is linear, as in the generated `Keyframe`. */
export type Keyframe = { time: { value: number; timescale: number }; value: number; easing?: KeyframeEasing | null };
/** A static value or keyframes over clip-local time, as Rust's `Animatable<f64>`. */
export type AnimatedNumber = number | { type: 'keyframes'; keyframes: Keyframe[] };
export type AudioClip = {
  src: string;
  sourceStart: number;
  playbackRate: AnimatedNumber;
  volume: AnimatedNumber;
  muted: boolean;
  start: number;
  duration: number;
  loop?: boolean;
  pitch?: AnimatedNumber;
  preservePitch?: boolean;
  fadeIn?: number;
  fadeOut?: number;
  /** The group the clip's sound belongs to, for other clips' `duck.under`. */
  duckGroup?: string;
  /** How the clip ducks, as authored on `<Audio duck>`. */
  duck?: AudioDuck;
};
export type Frame = { scene: Scene; audio: AudioClip[] };
