// Re-exports the render-output side of the Scene/Layer JSON contract from
// the bindings ts-rs generates from `mikan_composition` (see ../README.md
// for the regeneration command). `CompositionConfig` has no Rust
// counterpart: it is this package's own `{"config": ...}` CLI handshake
// message, not part of the Scene contract itself.

export type { AssetLocation } from './generated/AssetLocation';
export type { EvaluatedTransform } from './generated/EvaluatedTransform';
export type { Layer } from './generated/Layer';
export type { LayerContent } from './generated/LayerContent';
export type { Paint } from './generated/Paint';
export type { Point } from './generated/Point';
export type { Rational } from './generated/Rational';
export type { ResolvedAsset } from './generated/ResolvedAsset';
export type { Scene } from './generated/Scene';
export type { Stroke } from './generated/Stroke';
export type { TextAlign } from './generated/TextAlign';
export type { TextStyle } from './generated/TextStyle';
export type { Time } from './generated/Time';

import type { Rational } from './generated/Rational';

export interface CompositionConfig {
  width: number;
  height: number;
  frameRate: Rational;
  durationInFrames: number;
}
