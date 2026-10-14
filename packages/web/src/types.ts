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
export type LineDash = { pattern: number[]; offset?: number };
export type Point = { x: number; y: number };
export type Transform = { position: Point; scale: Point; rotation: number; anchor: Point };
export type PathCommand =
  | { type: 'moveTo' | 'lineTo'; x: number; y: number }
  | { type: 'quadTo'; x1: number; y1: number; x: number; y: number }
  | { type: 'cubicTo'; x1: number; y1: number; x2: number; y2: number; x: number; y: number }
  | { type: 'close' };
/** A family name, or a font stack tried in order. */
export type FontFamily = string | readonly string[];
export type FontRun = { start: number; end: number; fontWeight?: number | null; fontFamily?: FontFamily | null };
export type TextStyle = {
  lang?: string | null;
  fontFamily?: FontFamily | null;
  fontSize?: number | null;
  fontWeight?: number | null;
  fill?: Paint | null;
  stroke?: Stroke | null;
  align?: 'left' | 'center' | 'right' | null;
  lineHeight?: number | null;
  letterSpacing?: number | { em: number } | null;
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
  }
  | { type: 'rect'; width: number; height: number; fill?: Paint | null; stroke?: Stroke | null; cornerRadius: number }
  | { type: 'path'; commands: PathCommand[]; fill?: Paint | null; stroke?: Stroke | null; lineCap?: 'butt' | 'round' | 'square'; lineJoin?: 'miter' | 'round' | 'bevel'; miterLimit?: number; lineDash?: LineDash | null }
  | { type: 'text'; text: string; style: TextStyle; maxWidth?: number | null; baselineAnchor?: boolean }
  | { type: 'image'; asset: Asset; width?: number; height?: number; fit?: 'contain' | 'cover'; animation?: { sourceTimeSeconds: number; loop: boolean } }
  | { type: 'video'; asset: Asset; timing: { sourceTimeSeconds: number; loop?: boolean } }
  | { type: 'psd'; asset: Asset }
  | { type: 'missingComponent'; component: string; props: Record<string, unknown> };
/** A built-in color filter; see `LayerFilter` in `@celesta/react`. */
export type ColorFilter =
  | { type: 'brightness' | 'contrast' | 'saturation' | 'grayscale' | 'invert'; amount: number }
  | { type: 'hue'; degrees: number }
  | { type: 'tint'; color: string; amount: number }
  | { type: 'vignette'; amount: number; radius: number; softness: number };
export type Layer = {
  id: string;
  transform: Transform;
  opacity: number;
  effects?: {
    blur?: number;
    shadow?: { color: string; blur: number; offsetX: number; offsetY: number } | null;
    glow?: { color: string; blur: number } | null;
    filters?: (ColorFilter | { type: 'shader'; id: string })[];
  };
  blendMode?: 'normal' | 'multiply' | 'screen' | 'overlay' | 'add' | 'difference';
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
export type AudioClip = {
  src: string;
  sourceStart: number;
  playbackRate: number | { type: 'keyframes'; keyframes: unknown[] };
  volume: number | { type: 'keyframes'; keyframes: unknown[] };
  muted: boolean;
  start: number;
  duration: number;
  loop?: boolean;
};
export type Frame = { scene: Scene; audio: AudioClip[] };
