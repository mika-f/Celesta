// The turntable plugin's component, imported by entries as
// `@celesta-plugin/turntable`. `<Turntable>` puts one layer in the scene:
// plugin content for plugin `turntable`, which the plugin's process
// (src/main.rs) turns into a 3D scene. The layer is 640x360.
//
// Like everything in a composition, the layer depends only on the frame: its
// `time` is the enclosing sequence's, and the cube's turn is a function of it.

import * as React from 'react';

import { PluginLayer } from '@celesta.studio/react';
import type { CommonProps } from '@celesta.studio/react';

export interface TurntableProps extends CommonProps {
  /** Turns of the cube each second; negative turns it the other way. Default 0.5. */
  speed?: number;
}

/** The layer the plugin draws: no assets, the speed as data. */
export function Turntable({ speed, ...common }: TurntableProps): React.ReactElement {
  return React.createElement(PluginLayer, {
    ...common,
    plugin: 'turntable',
    ...(speed !== undefined ? { data: { speed } } : {}),
  });
}
