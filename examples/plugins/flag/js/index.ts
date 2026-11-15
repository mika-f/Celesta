// The flag plugin's component, imported by entries as `@celesta-plugin/flag`.
// `<Flag>` puts one layer in the scene: plugin content for plugin `flag`,
// which the plugin's process (src/main.rs) turns into meshes. The layer is
// the image's size plus room above and below for the wave.
//
// Like everything in a composition, the layer depends only on the frame: its
// `time` is the enclosing sequence's, and the wave is a function of it.

import * as React from 'react';

import { PluginLayer } from '@celesta.studio/react';
import type { AssetInput, CommonProps } from '@celesta.studio/react';

export interface FlagProps extends CommonProps {
  /** A PNG or JPEG. Its left edge is the pole. */
  src: AssetInput;
  /** How far the free edge swings, as a fraction of the image's height, up to 0.15. Default 0.06. */
  amplitude?: number;
  /** Wavelengths across the image. Default 1.5. */
  waves?: number;
  /** Waves passing a point each second. Default 0.8. */
  speed?: number;
  /** How much the folds darken and lighten the cloth, 0 to 1. Default 0.3. */
  shading?: number;
}

/** The layer the plugin draws: the image as asset `image`, the wave as data. */
export function Flag({ src, amplitude, waves, speed, shading, ...common }: FlagProps): React.ReactElement {
  const data: Record<string, number> = {};
  for (const [key, value] of Object.entries({ amplitude, waves, speed, shading })) {
    if (value !== undefined) data[key] = value;
  }
  return React.createElement(PluginLayer, {
    ...common,
    plugin: 'flag',
    assets: { image: src },
    ...(Object.keys(data).length ? { data } : {}),
  });
}
