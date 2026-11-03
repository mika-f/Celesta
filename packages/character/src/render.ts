// How the scene walker draws the character host elements the components in
// components.ts render. Importing this module registers them with the core
// renderer, which otherwise rejects these element types.

import type { ReactNode } from 'react';

import type { EvaluatedTransform, Layer, LayerContent } from '@celesta.studio/react';
import { flattenTextContent, registerHostElement } from '@celesta.studio/react/internal';
import type { HostNode, HostVisit, HostWalk } from '@celesta.studio/react/internal';

import { type BlinkPhase, type BlinkTiming, blinkPhase } from './blink';
import type { PsdCharacterBlink, PsdCharacterLipSync, PsdExpression } from './components';
import { resolveVisibleLayers } from './psd-preset';

function identity(): EvaluatedTransform {
  return { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0, y: 0 } };
}

/** Per frame: the `<Dialogue>` props driving each `<CharacterView>` host node. */
const OVERRIDES = Symbol('characterViewOverrides');

function overrides(visit: HostVisit): Map<HostNode, Record<string, unknown>> {
  let map = visit.frameState.get(OVERRIDES) as Map<HostNode, Record<string, unknown>> | undefined;
  if (!map) {
    map = new Map();
    visit.frameState.set(OVERRIDES, map);
  }
  return map;
}

function resolveReference(value: unknown): unknown {
  return value && typeof value === 'object' && 'current' in value
    ? (value as { current?: unknown }).current
    : value;
}

/** The timing fields of `value` that are set, so spreading it never clears a fallback. */
function definedTiming(value: BlinkTiming): BlinkTiming {
  const timing: BlinkTiming = {};
  if (value.interval !== undefined) timing.interval = value.interval;
  if (value.duration !== undefined) timing.duration = value.duration;
  if (value.seed !== undefined) timing.seed = value.seed;
  return timing;
}

function resolveCharacterView(value: unknown): HostNode | null {
  const node = resolveReference(value);
  return node !== null && typeof node === 'object' && (node as HostNode).type === 'character-view'
    ? (node as HostNode)
    : null;
}

function characterViewContent(node: HostNode, walk: HostWalk): LayerContent {
  const { props } = node;
  const override = overrides(walk).get(node);
  const character = resolveReference(props.character) as
    | {
        id?: string;
        portrait?:
          | {
              type?: 'image';
              defaultExpression: string;
              expressions: Record<string, unknown>;
              lipSync?: {
                a: unknown;
                i: unknown;
                u: unknown;
                e: unknown;
                o: unknown;
                closed?: unknown;
              };
              blink?: BlinkTiming & {
                closed: Record<string, unknown>;
                half?: Record<string, unknown>;
                overlay?: boolean;
              };
            }
          | {
              type: 'psd';
              src: unknown;
              layers?: string[] | string;
              hide?: string[] | string;
              expressions?: Record<string, PsdExpression>;
              defaultExpression?: string;
              lipSync?: PsdCharacterLipSync;
              blink?: PsdCharacterBlink;
            };
      }
    | null;
  const portrait = character?.portrait;
  if (!portrait) {
    throw new Error('<CharacterView> requires a character with a portrait');
  }
  const mouthValue = override?.mouth ?? props.mouth;
  const mouth = typeof mouthValue === 'string' ? mouthValue : undefined;
  // Blinks follow the composition frame, not the local one, so a cut to a
  // new <Sequence> does not restart (and re-sync) every character's rhythm.
  const eyes = (timing: BlinkTiming): BlinkPhase => {
    if (props.blink === false) {
      return 'open';
    }
    const viewTiming = typeof props.blink === 'object' && props.blink !== null ? (props.blink as BlinkTiming) : {};
    return blinkPhase(walk.frame, walk.fps, {
      seed: character?.id,
      ...definedTiming(timing),
      ...definedTiming(viewTiming),
    });
  };
  if (portrait.type === 'psd') {
    const expressionValue = override?.expression ?? props.expression;
    const expressionName = typeof expressionValue === 'string' ? expressionValue : portrait.defaultExpression;
    let expression: PsdExpression | undefined;
    if (expressionName !== undefined) {
      expression = portrait.expressions?.[expressionName];
      if (expression === undefined) {
        throw new Error(`character has no expression "${expressionName}"`);
      }
    }
    // An expression is its layers, or `{ layers, hide, lipSync, blink }` with
    // layers to hide or mouths or eyes of its own.
    const {
      layers: expressionLayers,
      hide: expressionHide,
      lipSync: expressionLipSync,
      blink: expressionBlink,
    } = typeof expression === 'object' && !Array.isArray(expression)
      ? expression
      : { layers: expression, hide: undefined, lipSync: undefined, blink: undefined };
    const lipSync = expressionLipSync ?? portrait.lipSync;
    // An expression's eyes replace the portrait's as a set, so a half-shut
    // layer from another face folder never leaks in; timing still falls back.
    const blink: PsdCharacterBlink | undefined =
      expressionBlink === false
        ? undefined
        : expressionBlink
          ? { ...definedTiming(portrait.blink ?? {}), ...expressionBlink }
          : portrait.blink;
    const selectedLayer = mouth && lipSync
      ? mouth === 'closed'
        ? lipSync.closed
        : lipSync[mouth as 'a' | 'i' | 'u' | 'e' | 'o']
      : undefined;
    const mouthLayers = lipSync ? Object.values(lipSync) : [];
    const layerList = (layers: string[] | string | undefined) =>
      Array.isArray(layers) ? layers : typeof layers === 'string' ? resolveVisibleLayers(layers) : [];
    // Eye and hidden paths are layer paths, never layer-state strings: a bare string is one path.
    const pathList = (layers: string[] | string | undefined) =>
      layers === undefined ? [] : Array.isArray(layers) ? layers : [layers];
    // Without `layers` the PSD's saved visibility is the base, and the
    // expression's layers are forced on top of it instead of replacing it.
    const poseLayers = layerList(portrait.layers);
    const faceLayers = layerList(expressionLayers);
    const visibleLayers = poseLayers.length ? [...new Set([...poseLayers, ...faceLayers])] : [];
    const enabledLayers = poseLayers.length ? [] : [...faceLayers];
    if (selectedLayer) {
      enabledLayers.push(selectedLayer);
    }
    const disabledLayers = selectedLayer ? mouthLayers.filter((layer) => layer !== selectedLayer) : [];
    // Hidden layers and folders never hide the expression's own layers, nor the
    // mouth and eyes lip sync and blinking show.
    const hidden = [...pathList(portrait.hide), ...pathList(expressionHide)];
    if (poseLayers.length) {
      // A preset's layers lose to a hidden folder; forced ones win over it.
      enabledLayers.push(...faceLayers.filter((layer) => hidden.some((folder) => layer.startsWith(`${folder}/`))));
    }
    if (blink) {
      const open = pathList(blink.open);
      const closed = pathList(blink.closed);
      const half = pathList(blink.half);
      let phase = eyes(blink);
      if (phase === 'half' && half.length === 0) {
        phase = 'open';
      }
      const shown = phase === 'closed' ? closed : phase === 'half' ? half : open;
      enabledLayers.push(...shown);
      disabledLayers.push(...[...open, ...closed, ...half].filter((layer) => !shown.includes(layer)));
    }
    disabledLayers.push(...hidden.filter((layer) => !faceLayers.includes(layer) && !enabledLayers.includes(layer)));
    return {
      type: 'psd',
      asset: walk.resolveAsset(portrait.src),
      ...(visibleLayers.length ? { visibleLayers } : {}),
      ...(enabledLayers.length ? { enabledLayers: [...new Set(enabledLayers)] } : {}),
      ...(disabledLayers.length ? { disabledLayers: [...new Set(disabledLayers)] } : {}),
    };
  } else {
    const expressionValue = override?.expression ?? props.expression;
    const expression = typeof expressionValue === 'string' ? expressionValue : portrait.defaultExpression;
    const src = portrait.expressions[expression];
    if (src === undefined) {
      throw new Error(`character has no expression "${expression}"`);
    }
    // An expression blinks only if it has an eyes-shut image.
    const blink = portrait.blink;
    const closedSource = blink?.closed[expression];
    let eyesSource: unknown;
    if (blink && closedSource !== undefined) {
      const phase = eyes(blink);
      eyesSource = phase === 'closed' ? closedSource : phase === 'half' ? blink.half?.[expression] : undefined;
    }
    const overlay = blink?.overlay === true;
    const layers: Layer[] = [
      {
        id: [walk.id, 'portrait'].join('.'),
        transform: identity(),
        opacity: 1,
        content: { type: 'image', asset: walk.resolveAsset(eyesSource !== undefined && !overlay ? eyesSource : src) },
      },
    ];
    if (eyesSource !== undefined && overlay) {
      layers.push({
        id: [walk.id, 'eyes'].join('.'),
        transform: identity(),
        opacity: 1,
        content: { type: 'image', asset: walk.resolveAsset(eyesSource) },
      });
    }
    const mouthSource = mouth && portrait.lipSync
      ? mouth === 'closed'
        ? portrait.lipSync.closed
        : portrait.lipSync[mouth as 'a' | 'i' | 'u' | 'e' | 'o']
      : undefined;
    if (mouthSource !== undefined) {
      layers.push({
        id: [walk.id, 'mouth'].join('.'),
        transform: identity(),
        opacity: 1,
        content: { type: 'image', asset: walk.resolveAsset(mouthSource) },
      });
    }
    return { type: 'group', layers };
  }
}

function dialogueContent(node: HostNode, walk: HostWalk): LayerContent {
  const { props } = node;
  const view = resolveCharacterView(props.character);
  if (!view && walk.frozen) {
    throw new Error(
      '<Dialogue> inside <FreezeFrame>, <MotionBlur>, or a <Trail> copy must refer to a <CharacterView> rendered inside the same copy',
    );
  }
  const character = resolveReference(view?.props.character) as
    | {
        portrait?: Record<string, unknown>;
        subtitle?: Record<string, unknown>;
      }
    | null;
  if (!character) {
    throw new Error('<Dialogue> requires a declared character');
  }
  const layers: Layer[] = [];
  if (typeof character.subtitle?.render === 'function') {
    // <Dialogue> already rendered the subtitle as its only child.
    for (const child of node.children) {
      layers.push(...walk.layers(child, [walk.id, 'subtitle'].join('.')));
    }
  } else if (props.held !== true) {
    // `${id}.subtitle` is already final (prefixed inside a <FreezeFrame>),
    // so it goes in as the path rather than as an authored id.
    const { emphasisStyles: _emphasisStyles, ...subtitle } = character.subtitle ?? {};
    layers.push(
      ...walk.layers(
        {
          type: 'text',
          props: {
            ...subtitle,
            ...(props.subtitleStyle !== undefined ? { style: props.subtitleStyle } : {}),
            id: undefined,
            children: props.text,
          },
          children: [],
          parent: null,
          changedAt: 0,
        },
        [walk.id, 'subtitle'].join('.'),
      ),
    );
  }
  return { type: 'group', layers };
}

// A declaration only; views read the character through its ref.
registerHostElement('asset-character', {});

registerHostElement('character-view', { content: characterViewContent });

registerHostElement('dialogue', {
  // A line drives its view wherever the view is drawn, even earlier in the tree.
  prepare(node, visit) {
    const view = resolveCharacterView(node.props.character);
    if (view) {
      overrides(visit).set(view, node.props);
    }
  },
  audio(node, walk) {
    if (node.props.audio !== undefined) {
      // A voice marks speech for ducking ("voice" unless the line says
      // otherwise, `null` for no group) and is never ducked itself.
      const duckGroup = node.props.duckGroup === undefined ? 'voice' : node.props.duckGroup;
      walk.collectAudio({ ...node.props, src: node.props.audio, duckGroup, duck: undefined });
    }
  },
  // The line is the subtitle for as long as it is up, or a page of it for
  // the page's frames; a held line, or a frame no page covers, is not.
  subtitles(node, walk) {
    if (node.props.held === true) return;
    const cue = node.props.subtitleCue as { text: string; from: number; to: number } | undefined;
    if (cue) {
      walk.collectSubtitle(cue.text, cue.from, cue.to);
    } else {
      walk.collectSubtitle(flattenTextContent(node.props.text as ReactNode, '<Dialogue> children').text);
    }
  },
  content: dialogueContent,
});
