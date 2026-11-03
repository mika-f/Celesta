// The flag plugin's component, imported by entries as `@celesta-plugin/flag`.
// `<Flag>` puts one layer in the scene: plugin content for plugin `flag`,
// which the plugin's process (src/main.rs) turns into meshes. The layer is
// the image's size plus room above and below for the wave.
//
// Like everything in a composition, the layer depends only on the frame: its
// `time` is the enclosing sequence's, and the wave is a function of it.

import * as React from 'react';

import { useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import type { AssetInput, CommonProps, LayerContent } from '@celesta.studio/react';
import { registerHostElement } from '@celesta.studio/react/internal';
import type { HostNode, HostWalk } from '@celesta.studio/react/internal';

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

const ELEMENT = 'celesta-plugin-flag';

export function Flag(props: FlagProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return React.createElement(ELEMENT, { ...props, time: frame / fps });
}

/** The layer the plugin draws: the image as asset `image`, the wave as data. */
function flagContent(node: HostNode, walk: HostWalk): LayerContent {
  const props = node.props as unknown as FlagProps & { time: number };
  const data: Record<string, number> = {};
  for (const key of ['amplitude', 'waves', 'speed', 'shading'] as const) {
    if (props[key] !== undefined) data[key] = props[key];
  }
  return {
    type: 'plugin',
    plugin: 'flag',
    assets: { image: walk.resolveAsset(props.src) },
    time: props.time,
    ...(Object.keys(data).length ? { data } : {}),
  };
}

registerHostElement(ELEMENT, { content: flagContent });
