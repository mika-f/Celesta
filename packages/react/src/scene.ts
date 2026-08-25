// Mirrors the JSON shape of `mikan_composition::Scene` and the types it
// re-exports (`crates/composition/src/model.rs`). Field names use the same
// camelCase serde rename as the Rust side, so these types can be produced and
// consumed with plain `JSON.stringify`/`JSON.parse` on both ends.

export interface Rational {
  numerator: number;
  denominator: number;
}

export interface Time {
  value: number;
  timescale: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface EvaluatedTransform {
  position: Point;
  scale: Point;
  /** Clockwise rotation in degrees. */
  rotation: number;
  /** Normalized coordinates where `(0.5, 0.5)` is the center. */
  anchor: Point;
}

export type AssetLocation = { type: 'file'; path: string } | { type: 'url'; url: string };

export interface ResolvedAsset {
  id: string;
  location: AssetLocation;
}

export type Paint = { type: 'solid'; color: string };

export interface Stroke {
  paint: Paint;
  width: number;
}

export type TextAlign = 'left' | 'center' | 'right';

export interface TextStyle {
  fontFamily?: string;
  fontSize?: number;
  fontWeight?: number;
  fill?: Paint;
  stroke?: Stroke;
  align?: TextAlign;
  lineHeight?: number;
}

export type LayerContent =
  | { type: 'group'; layers: Layer[] }
  | { type: 'image'; asset: ResolvedAsset }
  | { type: 'text'; text: string; style: TextStyle; maxWidth?: number };

export interface Layer {
  id: string;
  transform: EvaluatedTransform;
  opacity: number;
  content: LayerContent;
}

export interface Scene {
  width: number;
  height: number;
  frameRate: Rational;
  time: Time;
  fonts?: ResolvedAsset[];
  layers: Layer[];
}

export interface CompositionConfig {
  width: number;
  height: number;
  frameRate: Rational;
  durationInFrames: number;
}
