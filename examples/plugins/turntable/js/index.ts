// The turntable plugin's component, imported by entries as
// `@celesta-plugin/turntable`. `<Turntable>` puts one layer in the scene:
// plugin content for plugin `turntable`, which the plugin's process
// (src/main.rs) turns into a 3D scene. The layer is 640x360.
//
// Like everything in a composition, the layer depends only on the frame: its
// `time` is the enclosing sequence's, and the cube's turn is a function of it.

import * as React from 'react';

import { useCurrentFrame, useVideoConfig } from '@celesta/react';
import type { CommonProps, LayerContent } from '@celesta/react';
import { registerHostElement } from '@celesta/react/internal';
import type { HostNode } from '@celesta/react/internal';

export interface TurntableProps extends CommonProps {
  /** Turns of the cube each second; negative turns it the other way. Default 0.5. */
  speed?: number;
}

const ELEMENT = 'celesta-plugin-turntable';

export function Turntable(props: TurntableProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return React.createElement(ELEMENT, { ...props, time: frame / fps });
}

/** The layer the plugin draws: no assets, the speed as data. */
function turntableContent(node: HostNode): LayerContent {
  const props = node.props as unknown as TurntableProps & { time: number };
  return {
    type: 'plugin',
    plugin: 'turntable',
    time: props.time,
    ...(props.speed !== undefined ? { data: { speed: props.speed } } : {}),
  };
}

registerHostElement(ELEMENT, { content: turntableContent });
