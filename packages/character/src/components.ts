// The character components: a `<Character>` asset declaration, the
// `<CharacterView>` that draws its portrait, and the `<Dialogue>` lines that
// drive a view's mouth and expression and show its subtitle. Each renders a
// host element that render.ts registers with the core scene walker.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group } from '@celesta/react';
import type { AnimatedNumber, AssetInput, AssetReference, CommonProps, TextMetrics, TextStyle } from '@celesta/react';
import {
  CompositionRuntimeContext,
  FreezeFrameContext,
  RerenderRequestContext,
  flattenTextContent,
  secondsFromTime,
  synchronousMeasurer,
  useMeasurementFonts,
  withTextLanguage,
  withTextRuns,
} from '@celesta/react/internal';
import type { MeasureTextRequest } from '@celesta/react/internal';

import type { BlinkTiming } from './blink';
import { useOptionalLipSync } from './lipsync';
import type { LipSyncTrack } from './lipsync';
import './render';

/** A declared character, as its `<Character>` ref holds it. */
export interface CharacterReference extends AssetReference {
  readonly kind: 'character';
  readonly portrait?: CharacterPortrait;
  readonly subtitle?: CharacterSubtitle;
  /** The character's `name`. */
  readonly name: string;
  /** The character's `displayName`, or its `name` when not given. */
  readonly displayName: string;
}

export interface CharacterProps {
  name: string;
  /**
   * The name shown to viewers, such as on a subtitle's name plate (see
   * `SubtitleRenderProps.character`). Defaults to `name`.
   */
  displayName?: string;
  id?: string;
  portrait?: CharacterPortrait;
  subtitle?: CharacterSubtitle;
  children?: ReactNode;
}

export interface ImageCharacterPortrait extends CommonProps {
  type?: 'image';
  defaultExpression: string;
  expressions: Record<string, AssetInput>;
  lipSync?: CharacterLipSync;
  /** Eyes-shut images to blink with; see `ImageCharacterBlink`. */
  blink?: ImageCharacterBlink;
}

/**
 * Blinking for an image portrait. Images are keyed by expression; an
 * expression without a `closed` image does not blink.
 */
export interface ImageCharacterBlink extends BlinkTiming {
  /** The eyes-shut image for each expression that blinks. */
  closed: Record<string, AssetInput>;
  /** Optional half-shut images, shown on the frames around each blink. */
  half?: Record<string, AssetInput>;
  /**
   * `false` (default): the images are whole portraits that replace the
   * expression's image while the eyes are shut. `true`: they are transparent
   * eye images the same size as the portrait, drawn over it like lip-sync
   * mouths.
   */
  overlay?: boolean;
}

export interface PsdCharacterPortrait extends CommonProps {
  type: 'psd';
  src: AssetInput;
  /**
   * Which layers compose the base portrait. Real multi-outfit / multi-face
   * PSDs save every folder hidden, so without this only the lip-sync mouth
   * would render. Either the flat list of visible layer/folder full paths
   * (PSDTool "all layer" semantics), or a raw PSDTool layer-state string to
   * parse (paste "copy layer state" output directly). For a `.pfv` file,
   * resolve it in `prepare()` with `loadPsdPreset()` and pass the result
   * here. When omitted the PSD renders from its own saved visibility.
   */
  layers?: string[] | string;
  /**
   * Named expressions. Each is the layers shown on top of `layers` (which
   * every expression shares) — a list of layer paths, or a PSDTool layer-state
   * string — or `{ layers, lipSync }` for a PSD whose expressions have mouths
   * of their own. `<CharacterView expression>` and `<Dialogue expression>`
   * pick one; otherwise `defaultExpression` is used, if given.
   */
  expressions?: Record<string, PsdExpression>;
  /** The expression shown when none is picked. Must be a key of `expressions`. */
  defaultExpression?: string;
  lipSync?: PsdCharacterLipSync;
  /** Eye layers to blink with; an expression's `blink` replaces them. */
  blink?: PsdCharacterBlink;
}

/**
 * Blinking for a PSD portrait: the eye layers, by full path, and optionally
 * when to blink. The open layers are forced visible and the others hidden,
 * and while the eyes are shut it is the other way round, as for lip sync.
 */
export interface PsdCharacterBlink extends BlinkTiming {
  open: string | string[];
  closed: string | string[];
  /** Half-shut eyes, shown on the frames around each blink. */
  half?: string | string[];
}

/** One expression of a PSD portrait; see `PsdCharacterPortrait.expressions`. */
export type PsdExpression =
  | string[]
  | string
  | {
      layers: string[] | string;
      /** Replaces the portrait's `lipSync` while this expression is shown. */
      lipSync?: PsdCharacterLipSync;
      /**
       * Replaces the portrait's eye layers while this expression is shown
       * (timing it omits still comes from the portrait's `blink`), or `false`
       * for an expression that does not blink, such as one with shut eyes.
       */
      blink?: PsdCharacterBlink | false;
    };

export type CharacterPortrait = ImageCharacterPortrait | PsdCharacterPortrait;

export interface CharacterLipSync {
  a: AssetInput;
  i: AssetInput;
  u: AssetInput;
  e: AssetInput;
  o: AssetInput;
  closed?: AssetInput;
}

export interface PsdCharacterLipSync {
  a: string;
  i: string;
  u: string;
  e: string;
  o: string;
  closed?: string;
}

export interface CharacterViewProps extends CommonProps {
  character: AssetInput;
  expression?: string;
  mouth?: 'closed' | 'a' | 'i' | 'u' | 'e' | 'o';
  /**
   * A lip-sync track from `loadLipSync()`. When set and `mouth` is not
   * given, the mouth shape is driven from the track for the current frame.
   */
  lipSync?: LipSyncTrack;
  /**
   * Blinking, when the portrait has `blink` configured. `false` holds the
   * eyes open (for a close-up, say); an object overrides the portrait's
   * timing, such as its `seed`. Default `true`.
   */
  blink?: boolean | BlinkTiming;
}

declare const characterViewReference: unique symbol;

/** Opaque reference to a mounted `<CharacterView>`. */
export interface CharacterViewReference {
  readonly [characterViewReference]: true;
}

export interface CharacterSubtitle extends CommonProps {
  style?: TextStyle;
  maxWidth?: number;
  /**
   * Draws the subtitle yourself, e.g. with a band behind the text and a name
   * plate. What it returns is placed like the children of a `<Group>` with
   * this subtitle's other layer props (`x`, `y`, `opacity`, …); `style` and
   * `maxWidth` are only used to measure the text for `metrics`. Called as a
   * component, so it may use hooks.
   */
  render?: (props: SubtitleRenderProps) => ReactNode;
}

/** The speaker of a subtitle, as given to `CharacterSubtitle.render`. */
export interface SubtitleCharacter {
  id: string;
  name: string;
  /** `displayName`, or `name` when the character has none. */
  displayName: string;
}

/** What `CharacterSubtitle.render` draws from. */
export interface SubtitleRenderProps {
  /** The line's text. */
  text: string;
  /** The line as given, spans included: draw it with `<Text style={style}>{content}</Text>`. */
  content: ReactNode;
  character: SubtitleCharacter;
  /** `text` measured with the subtitle's `style` and `maxWidth`, as `<Text>` lays it out. */
  metrics: TextMetrics;
  style: TextStyle;
  maxWidth?: number;
  /**
   * True while the line has ended but the subtitle is kept up until the next
   * one (`<Dialogue held>`, `<DialogueSeries holdSubtitle>`): draw the band,
   * not the text. `text` and `metrics` are still the line's, so a band
   * sized from them keeps its size.
   */
  held: boolean;
  /**
   * Frames since the subtitle appeared: since the enclosing `<Sequence>`
   * started, or under `<DialogueSeries holdSubtitle>` since the run of
   * lines it is held through started. Use it to fade the band in.
   */
  frame: number;
  /** Frames the subtitle stays up in all, counted like `frame`. Use it to fade the band out. */
  durationInFrames: number;
}
export interface DialogueProps extends CommonProps {
  character: React.RefObject<CharacterViewReference | null>;
  children: ReactNode;
  expression?: string;
  mouth?: CharacterViewProps['mouth'];
  lipSync?: LipSyncTrack;
  audio?: AssetInput;
  startFrom?: number;
  playbackRate?: AnimatedNumber;
  volume?: AnimatedNumber;
  muted?: boolean;
  /**
   * Keeps the subtitle up after its line has ended, until the next line: a
   * `subtitle.render` is called with `held: true`, and a plain subtitle
   * draws nothing. Give it no `audio`, `expression`, or `mouth`; it is not
   * the line itself. `<DialogueSeries holdSubtitle>` places these for you.
   */
  held?: boolean;
}

/** @internal Set by `<DialogueSeries holdSubtitle>`: where each line sits in its run. */
export const SubtitleRunContext = React.createContext<{ offset: number; durationInFrames: number } | null>(null);

// The ref is typed as the core `AssetReference`, like every other asset, so
// one `useRef<AssetReference>()` style works for all of them; it holds a
// `CharacterReference`.
export const Character = React.forwardRef<AssetReference, CharacterProps>(function Character(
  props,
  ref,
) {
  const reference = React.useMemo<CharacterReference>(
    () => ({
      id: props.id ?? props.name,
      kind: 'character',
      portrait: props.portrait,
      subtitle: props.subtitle,
      name: props.name,
      displayName: props.displayName ?? props.name,
    }),
    [props.id, props.name, props.displayName, props.portrait, props.subtitle],
  );
  // Assigned during render too, so a view rendered later in the same pass can read it.
  if (ref && typeof ref === 'object') {
    ref.current = reference;
  }
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement('asset-character', props);
});

/**
 * The views drawn inside one `<FreezeFrame>`, by the ref their author
 * passed. A copy never attaches the author's ref, which the live view owns;
 * `<Dialogue>`s inside resolve that ref through `references` instead.
 */
interface ViewScope {
  readonly views: Map<object, unknown>;
  readonly references: WeakMap<object, { readonly current: unknown }>;
}

/** Keyed by the scope object `<FreezeFrame>` provides through `FreezeFrameContext`. */
const viewScopes = new WeakMap<object, ViewScope>();

/** This `<FreezeFrame>`'s view scope, or null outside one. */
function useViewScope(): ViewScope | null {
  const freeze = React.useContext(FreezeFrameContext);
  if (!freeze) return null;
  let scope = viewScopes.get(freeze);
  if (!scope) {
    scope = { views: new Map(), references: new WeakMap() };
    viewScopes.set(freeze, scope);
  }
  return scope;
}

/** A ref-like object reading the copy of `ref`'s view in `scope`; stable per ref. */
function scopedViewReference(scope: ViewScope, ref: object): { readonly current: unknown } {
  let reference = scope.references.get(ref);
  if (!reference) {
    reference = {
      get current() {
        return scope.views.get(ref) ?? null;
      },
    };
    scope.references.set(ref, reference);
  }
  return reference;
}

export const CharacterView = React.forwardRef<CharacterViewReference, CharacterViewProps>(
  function CharacterView(props, ref) {
    const { lipSync, ...rest } = props;
    const tracked = useOptionalLipSync(lipSync);
    const mouth = rest.mouth ?? tracked;
    const scope = useViewScope();
    const attached = React.useRef<unknown>(null);
    // Detaching passes null, so remember which node this view registered and
    // only remove the entry while it is still ours.
    const register = React.useCallback(
      (node: unknown) => {
        if (!scope || !ref) return;
        if (node !== null) {
          attached.current = node;
          scope.views.set(ref, node);
        } else {
          if (scope.views.get(ref) === attached.current) scope.views.delete(ref);
          attached.current = null;
        }
      },
      [scope, ref],
    );
    return React.createElement('character-view', { ...rest, mouth, ref: scope ? register : ref });
  },
);

/** Renders a character portrait and its configured subtitle as one layer. */
export function Dialogue(props: DialogueProps): ReturnType<typeof React.createElement> {
  const { children, lipSync, mouth, ...rest } = props;
  const tracked = useOptionalLipSync(lipSync);
  const scope = useViewScope();
  const character = scope && props.character
    ? (scopedViewReference(scope, props.character) as DialogueProps['character'])
    : props.character;
  const subtitle = useRenderedSubtitle(character, children, props.held === true);
  return React.createElement(
    'dialogue',
    {
      ...rest,
      character,
      text: children,
      mouth: mouth ?? tracked,
    },
    subtitle,
  );
}

/**
 * The subtitle a `subtitle.render` draws for this line, or null for a
 * plain subtitle, which render.ts draws as a `text` layer.
 */
function useRenderedSubtitle(
  view: React.RefObject<CharacterViewReference | null>,
  children: ReactNode,
  held: boolean,
): ReactNode {
  const requestRerender = React.useContext(RerenderRequestContext);
  const runtime = React.useContext(CompositionRuntimeContext);
  const run = React.useContext(SubtitleRunContext);
  const fonts = useMeasurementFonts();
  // A view mounted in this same commit has no ref yet; render.ts reconciles
  // once more after the commit attaches it.
  const viewNode = view?.current as { props?: { character?: unknown } } | null | undefined;
  if (viewNode == null) {
    requestRerender();
  }
  const characterInput = viewNode?.props?.character;
  const character = (
    characterInput && typeof characterInput === 'object' && 'current' in characterInput
      ? characterInput.current
      : characterInput
  ) as CharacterReference | null | undefined;
  const subtitle = character?.subtitle;
  const render = subtitle?.render;
  const flat = render ? flattenTextContent(children, '<Dialogue> children') : null;
  const text = flat?.text ?? '';
  const style = withTextLanguage(subtitle?.style ?? {}, runtime?.lang);
  const maxWidth = subtitle?.maxWidth;
  const key = flat
    ? JSON.stringify({
      text,
      style: withTextRuns(style, flat, '<Dialogue>'),
      ...(maxWidth !== undefined ? { maxWidth } : {}),
      fonts,
    })
    : null;
  const metrics = React.useMemo(
    () => (key === null ? null : synchronousMeasurer('subtitle.render')(JSON.parse(key) as MeasureTextRequest)),
    [key],
  );
  if (!render || !character || !metrics || !runtime) {
    return null;
  }
  const localFrame = Math.round(secondsFromTime(runtime.time) * runtime.fps);
  const { style: _style, maxWidth: _maxWidth, render: _render, id: _id, ...placement } = subtitle;
  const name = character.name ?? character.id;
  const info: SubtitleRenderProps = {
    text,
    content: children,
    character: { id: character.id, name, displayName: character.displayName ?? name },
    metrics,
    style,
    ...(maxWidth !== undefined ? { maxWidth } : {}),
    held,
    frame: localFrame + (run?.offset ?? 0),
    durationInFrames: run?.durationInFrames ?? runtime.durationInFrames,
  };
  return React.createElement(Group, placement, React.createElement(SubtitleRenderer, { render, info }));
}

function SubtitleRenderer({
  render,
  info,
}: {
  render: (props: SubtitleRenderProps) => ReactNode;
  info: SubtitleRenderProps;
}): ReactNode {
  return render(info);
}
