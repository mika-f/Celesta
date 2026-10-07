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
export type Point = { x: number; y: number };
export type Transform = { position: Point; scale: Point; rotation: number; anchor: Point };
export type PathCommand =
  | { type: 'moveTo' | 'lineTo'; x: number; y: number }
  | { type: 'quadTo'; x1: number; y1: number; x: number; y: number }
  | { type: 'cubicTo'; x1: number; y1: number; x2: number; y2: number; x: number; y: number }
  | { type: 'close' };
export type TextStyle = {
  lang?: string | null;
  fontFamily?: string | null;
  fontSize?: number | null;
  fontWeight?: number | null;
  fill?: Paint | null;
  stroke?: Stroke | null;
  align?: 'left' | 'center' | 'right' | null;
  lineHeight?: number | null;
  letterSpacing?: number | null;
  lineBreak?: 'normal' | 'phrase' | null;
};
export type LayerContent =
  | { type: 'group'; layers: Layer[]; clip?: { x: number; y: number; width: number; height: number; cornerRadius: number } | null }
  | { type: 'rect'; width: number; height: number; fill?: Paint | null; stroke?: Stroke | null; cornerRadius: number }
  | { type: 'path'; commands: PathCommand[]; fill?: Paint | null; stroke?: Stroke | null; lineCap?: 'butt' | 'round' | 'square'; lineJoin?: 'miter' | 'round' | 'bevel'; miterLimit?: number }
  | { type: 'text'; text: string; style: TextStyle; maxWidth?: number | null; baselineAnchor?: boolean }
  | { type: 'image'; asset: Asset; width?: number; height?: number; fit?: 'contain' | 'cover' }
  | { type: 'video'; asset: Asset; timing: { sourceTimeSeconds: number } }
  | { type: 'psd'; asset: Asset }
  | { type: 'missingComponent'; component: string; props: Record<string, unknown> };
export type Layer = {
  id: string;
  transform: Transform;
  opacity: number;
  effects?: { blur?: number; shadow?: { color: string; blur: number; offsetX: number; offsetY: number } | null; glow?: { color: string; blur: number } | null };
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
};
export type Frame = { scene: Scene; audio: AudioClip[] };
