// `<PluginLayer>`: one layer a content plugin's process draws. A plugin's
// module renders it in place of a host element of its own, so plugin modules
// need nothing from `@celesta.studio/react/internal`. The layer is plugin
// content (`{ type: 'plugin', plugin, assets, time, data }`); the plugin's
// process turns it into meshes or a scene, and its transform, opacity, blend
// mode, and effects come from the common props, as any layer's do.

import * as React from 'react';

import type { AssetInput, CommonProps } from './components';
import type { JsonValue } from './generated/serde_json/JsonValue';
import { registerHostElement } from './host-elements';
import type { HostWalk } from './host-elements';
import { useCurrentFrame, useVideoConfig } from './hooks';
import type { HostNode } from './reconciler';
import type { LayerContent } from './scene';

export interface PluginLayerProps extends CommonProps {
  /** The id of the plugin that draws the layer, as its manifest gives it. */
  plugin: string;
  /** The files the plugin reads, by the names it knows them by. */
  assets?: Record<string, AssetInput>;
  /**
   * The time the plugin draws, in seconds. Defaults to the enclosing
   * sequence's: `useCurrentFrame() / fps`.
   */
  time?: number;
  /** What the plugin gets with the frame, as JSON: a pure function of the frame. */
  data?: JsonValue;
}

/** The host element `<PluginLayer>` renders. */
const ELEMENT = 'plugin-layer';

/** The props of the host element: the common ones and the plugin's. */
interface ElementProps extends Omit<PluginLayerProps, 'time'> {
  time: number;
}

export function PluginLayer(props: PluginLayerProps): React.ReactElement {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { plugin, time } = props;
  if (typeof plugin !== 'string' || plugin === '') {
    throw new Error('<PluginLayer> needs the id of the plugin that draws it');
  }
  if (time !== undefined && !Number.isFinite(time)) {
    throw new Error('<PluginLayer> time is a finite number of seconds');
  }
  return React.createElement(ELEMENT, { ...props, time: time ?? frame / fps } satisfies ElementProps);
}

function pluginLayerContent(node: HostNode, walk: HostWalk): LayerContent {
  const { plugin, assets, time, data } = node.props as unknown as ElementProps;
  const resolved = Object.entries(assets ?? {}).map(([name, src]) => [name, walk.resolveAsset(src)]);
  return {
    type: 'plugin',
    plugin,
    ...(resolved.length ? { assets: Object.fromEntries(resolved) } : {}),
    time,
    ...(data !== undefined ? { data } : {}),
  };
}

registerHostElement(ELEMENT, { content: pluginLayerContent });
