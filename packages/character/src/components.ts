// The character components: a `<Character>` asset declaration, the
// `<CharacterView>` that draws its portrait, and the `<Dialogue>` lines that
// drive a view's mouth and expression and show its subtitle. Each renders a
// host element that render.ts registers with the core scene walker.

import * as React from 'react';
import type { ReactNode } from 'react';

import { Group, useCurrentFrame, useCurrentTime, useVideoConfig } from '@celesta.studio/react';
import { emphasisContent, resolveEmphasis } from '@celesta.studio/text';
import type { EmphasisStyle, ResolvedEmphasis, TextEmphasis } from '@celesta.studio/text';
import type {
  AnimatedNumber, AssetInput, AssetReference, CommonProps, ResolvedTheme, TextMetrics, TextStyle,
} from '@celesta.studio/react';
import {
  CompositionRuntimeContext,
  FreezeFrameContext,
  RerenderRequestContext,
  ThemeContext,
  flattenTextContent,
  secondsFromTime,
  synchronousMeasurer,
  themeTextContent,
  themedTextStyle,
  useMeasurementFonts,
  withInheritedText,
  withTextRuns,
} from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';

import type { BlinkTiming } from './blink';
import { useOptionalLipSync } from './lipsync';
import type { LipSyncTrack } from './lipsync';
import { resolveSubtitlePages, shownSubtitle } from './subtitle-pages';
import type { SubtitlePage, SubtitlePageInfo } from './subtitle-pages';
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
   * here. When omitted the PSD renders from its own saved visibility, and
   * an expression's layers are shown on top of it.
   */
  layers?: string[] | string;
  /**
   * Layers or folders (full paths) to hide, such as a prop the saved
   * visibility shows. A folder hides every layer in it except the ones the
   * expression, lip sync, or blinking shows. An unknown path is an error.
   */
  hide?: string[] | string;
  /**
   * Named expressions. Each is the layers shown on top of `layers` (which
   * every expression shares) — a list of layer paths, or a PSDTool layer-state
   * string — or `{ layers, hide, lipSync, blink }` for an expression that
   * hides layers or has mouths or eyes of its own. `<CharacterView expression>` and `<Dialogue expression>`
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
      /**
       * Layers or folders (full paths) hidden while this expression is shown,
       * on top of the portrait's `hide`: a face folder such as `face/brows`
       * hides the brow the PSD was saved with, so only this expression's
       * brow in it shows.
       */
      hide?: string[] | string;
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
  /**
   * Emphasis styles by name, for the `emphasis` of this character's lines
   * whose `style` is a string, such as `{ danger: { color: '#ff6060',
   * fontWeight: 700 } }`.
   */
  emphasisStyles?: Readonly<Record<string, EmphasisStyle>>;
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
  /** The text shown: the line's, or with `subtitlePages` the current page's. */
  text: string;
  /**
   * `text` as it is drawn, spans and emphasis included: draw it with
   * `<Text style={style}>{content}</Text>`.
   */
  content: ReactNode;
  /** The whole line's text, whichever page is shown. */
  lineText: string;
  character: SubtitleCharacter;
  /**
   * `content` measured with the subtitle's `style` and `maxWidth`, as
   * `<Text>` lays it out. Measured when first read.
   */
  readonly metrics: TextMetrics;
  style: TextStyle;
  maxWidth?: number;
  /**
   * True while the line has ended but the subtitle is kept up until the next
   * one (`<Dialogue held>`, `<DialogueSeries holdSubtitle>`), and on frames
   * of a line no subtitle page covers: draw the band, not the text. `text`
   * and `metrics` are still the line's (the page shown last), so a band
   * sized from them keeps its size.
   */
  held: boolean;
  /**
   * The page shown, for a line with `subtitlePages`, else null: its index
   * and count, its frames on the line's clock, and `frame`, frames since it
   * started, for animating a page turn.
   */
  page: SubtitlePageInfo | null;
  /** The emphasis on `text`, checked, in order, and counted in its grapheme clusters. */
  emphasis: ResolvedEmphasis[];
  /**
   * Frames since the subtitle appeared: since the enclosing `<Sequence>`
   * started, or under `<DialogueSeries holdSubtitle>` since the run of
   * lines it is held through started. Use it to fade the band in.
   */
  frame: number;
  /** Frames the subtitle stays up in all, counted like `frame`. Use it to fade the band out. */
  durationInFrames: number;
}
/** A change of expression within a line; see `DialogueExpression`. */
export interface ExpressionCue {
  /** Seconds from the start of the line, the clock `lipSync` uses. */
  at: number;
  /** A key of the portrait's `expressions`. */
  expression: string;
}

/**
 * The expression a line gives its view: one name for the whole line, or
 * cues that change it partway through, in order of `at`. Before the first
 * cue the view shows its own expression; from each cue on, that cue's, up
 * to the line's end (through the gap under `holdThroughGap`).
 */
export type DialogueExpression = string | readonly ExpressionCue[];

/**
 * The expression `expression` picks `seconds` into its line, or undefined
 * when it picks none (the view's own shows). The same lookup `<Dialogue>`
 * does, for parts that follow the speaker's face.
 */
export function expressionAt(expression: DialogueExpression | undefined, seconds: number): string | undefined {
  if (expression === undefined || typeof expression === 'string') {
    return expression;
  }
  let current: string | undefined;
  for (const cue of checkedCues(expression)) {
    if (cue.at > seconds) break;
    current = cue.expression;
  }
  return current;
}

function checkedCues(cues: readonly ExpressionCue[]): readonly ExpressionCue[] {
  if (!Array.isArray(cues)) {
    throw new Error('expression must be a name or a list of { at, expression } cues');
  }
  let previous = 0;
  cues.forEach((cue, index) => {
    if (typeof cue?.expression !== 'string') {
      throw new Error(`expression cue ${index} needs an \`expression\` name`);
    }
    if (typeof cue.at !== 'number' || !Number.isFinite(cue.at) || cue.at < 0) {
      throw new Error(`expression cue ${index} ("${cue.expression}") needs \`at\` in seconds >= 0, got ${cue.at}`);
    }
    if (cue.at < previous) {
      throw new Error(`expression cue ${index} ("${cue.expression}") at ${cue.at} s comes before the cue before it (${previous} s); list cues in order`);
    }
    previous = cue.at;
  });
  return cues;
}

export interface DialogueProps extends CommonProps {
  character: React.RefObject<CharacterViewReference | null>;
  children: ReactNode;
  /** The view's expression while the line is up; cues change it partway through. */
  expression?: DialogueExpression;
  mouth?: CharacterViewProps['mouth'];
  lipSync?: LipSyncTrack;
  audio?: AssetInput;
  startFrom?: number;
  playbackRate?: AnimatedNumber;
  volume?: AnimatedNumber;
  muted?: boolean;
  /**
   * The group the voice belongs to, for other clips' `duck.under` (see
   * `<Audio duck>`). Defaults to `"voice"`; `null` leaves the voice out of
   * every group. A voice is never ducked itself.
   */
  duckGroup?: string | null;
  /**
   * Keeps the subtitle up after its line has ended, until the next line: a
   * `subtitle.render` is called with `held: true`, and a plain subtitle
   * draws nothing. Give it no `audio`, `expression`, or `mouth`; it is not
   * the line itself. `<DialogueSeries holdSubtitle>` places these for you.
   */
  held?: boolean;
  /**
   * The subtitle a page at a time, on the line's clock (frame 0 is the
   * enclosing `<Sequence>`'s start); see `SubtitlePage`. Needs the
   * children as one string. The voice and lip sync are not affected.
   */
  subtitlePages?: readonly SubtitlePage[];
  /**
   * Parts of the line's text to emphasize, in grapheme clusters, with styles
   * inline or named in the speaker's `subtitle.emphasisStyles`. Needs the
   * children as one string.
   */
  emphasis?: readonly TextEmphasis[];
}

/** @internal Set by `<DialogueSeries holdSubtitle>`: where each line sits in its run. */
export const SubtitleRunContext = React.createContext<{ offset: number; durationInFrames: number } | null>(null);

/**
 * @internal Set by `<DialogueSeries>`: the line a `<Dialogue>` shows, where
 * its frame 0 sits on the line's clock (later for a held copy), and how
 * long the line is up, which an open-ended last page runs to.
 */
export const DialogueLineContext = React.createContext<{ id: string; offset: number; durationInFrames: number } | null>(null);

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
 * The views drawn inside one `<FreezeFrame>`, `<MotionBlur>` sample, or
 * `<Trail>` copy, by the ref their author passed. A copy never attaches the
 * author's ref, which the live view owns; `<Dialogue>`s inside resolve that
 * ref through `references` instead.
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
  const { children, lipSync, mouth, expression, subtitlePages, emphasis, ...rest } = props;
  const tracked = useOptionalLipSync(lipSync);
  const time = useCurrentTime();
  const scope = useViewScope();
  const character = scope && props.character
    ? (scopedViewReference(scope, props.character) as DialogueProps['character'])
    : props.character;
  const theme = React.useContext(ThemeContext);
  const speaker = useSpeaker(character);
  const shown = useShownSubtitle(children, subtitlePages, emphasis, speaker, theme);
  const held = props.held === true || !shown.covered;
  const [subtitle, subtitleStyle] = useRenderedSubtitle(speaker, shown, held, theme);
  return React.createElement(
    'dialogue',
    {
      ...rest,
      character,
      // A plain subtitle is drawn by render.ts, past every <Theme>, so it
      // gets the text and style with their names already looked up.
      text: themeTextContent(shown.content, theme),
      held,
      ...(shown.cue ? { subtitleCue: shown.cue } : {}),
      ...(subtitleStyle !== undefined ? { subtitleStyle } : {}),
      mouth: mouth ?? tracked,
      expression: expressionAt(expression, time.value / time.timescale),
    },
    subtitle,
  );
}

/**
 * The character a `<Dialogue>`'s view shows. A view mounted in this same
 * commit has no ref yet; render.ts reconciles once more after the commit
 * attaches it.
 */
function useSpeaker(view: React.RefObject<CharacterViewReference | null>): CharacterReference | null | undefined {
  const requestRerender = React.useContext(RerenderRequestContext);
  const viewNode = view?.current as { props?: { character?: unknown } } | null | undefined;
  if (viewNode == null) {
    requestRerender();
  }
  const characterInput = viewNode?.props?.character;
  return (
    characterInput && typeof characterInput === 'object' && 'current' in characterInput
      ? characterInput.current
      : characterInput
  ) as CharacterReference | null | undefined;
}

/** What a `<Dialogue>`'s subtitle shows on this frame. */
interface ShownSubtitle {
  /** Drawn as the subtitle: the children, or the page with its emphasis as spans. */
  content: ReactNode;
  /** The whole line's text, when it is one string. */
  lineText: string | null;
  page: SubtitlePageInfo | null;
  emphasis: ResolvedEmphasis[];
  /** False on a frame of the line no page covers: draw the band, not the text. */
  covered: boolean;
  /** The page for the subtitle file, in seconds of the enclosing `<Sequence>`. */
  cue?: { text: string; from: number; to: number };
}

function hasNamedStyle(emphasis: readonly TextEmphasis[] | undefined): boolean {
  return Array.isArray(emphasis) && emphasis.some((entry) => typeof entry?.style === 'string');
}

function useShownSubtitle(
  children: ReactNode,
  pages: readonly SubtitlePage[] | undefined,
  emphasis: readonly TextEmphasis[] | undefined,
  speaker: CharacterReference | null | undefined,
  theme: ResolvedTheme,
): ShownSubtitle {
  const line = React.useContext(DialogueLineContext);
  const { fps, durationInFrames } = useVideoConfig();
  const frame = useCurrentFrame();
  if (pages === undefined && emphasis === undefined) {
    return { content: children, lineText: typeof children === 'string' ? children : null, page: null, emphasis: [], covered: true };
  }
  const what = line ? `line "${line.id}"` : '<Dialogue>';
  if (typeof children !== 'string') {
    throw new Error(`${what}: subtitlePages and emphasis need the line's text as one string; style it with emphasis instead of <Span>`);
  }
  // Named styles are the speaker's, known once its view is attached.
  const resolved = speaker || !hasNamedStyle(emphasis)
    ? resolveEmphasis(children, emphasis, { styles: speaker?.subtitle?.emphasisStyles, theme, what })
    : [];
  if (pages === undefined) {
    return { content: emphasisContent(children, resolved), lineText: children, page: null, emphasis: resolved, covered: true };
  }
  const offset = line?.offset ?? 0;
  const resolvedPages = resolveSubtitlePages(children, pages, line?.durationInFrames ?? durationInFrames, what);
  const lineFrame = frame + offset;
  const shown = shownSubtitle(resolvedPages, lineFrame, resolved);
  const { page } = shown;
  return {
    content: emphasisContent(page.text, shown.emphasis),
    lineText: children,
    page: {
      index: page.index,
      count: resolvedPages.length,
      from: page.from,
      durationInFrames: page.end - page.from,
      frame: lineFrame - page.from,
      text: page.text,
      timing: page.timing,
    },
    emphasis: shown.emphasis,
    covered: shown.covered,
    ...(shown.covered ? { cue: { text: page.text, from: (page.from - offset) / fps, to: (page.end - offset) / fps } } : {}),
  };
}

/**
 * The subtitle a `subtitle.render` draws for this line, or null for a
 * plain subtitle, which render.ts draws as a `text` layer; and the
 * subtitle's style with `theme`'s names looked up, when that changes it.
 */
function useRenderedSubtitle(
  character: CharacterReference | null | undefined,
  shown: ShownSubtitle,
  held: boolean,
  theme: ResolvedTheme,
): [ReactNode, TextStyle | undefined] {
  const runtime = React.useContext(CompositionRuntimeContext);
  const run = React.useContext(SubtitleRunContext);
  const fonts = useMeasurementFonts();
  const subtitle = character?.subtitle;
  const render = subtitle?.render;
  const themedStyle = themedTextStyle(subtitle?.style, {}, theme, '<Dialogue> subtitle');
  const flat = render ? flattenTextContent(themeTextContent(shown.content, theme), '<Dialogue> children') : null;
  const text = flat?.text ?? '';
  const style = withInheritedText(themedStyle ?? {}, runtime ?? {});
  const maxWidth = subtitle?.maxWidth;
  const key = flat
    ? JSON.stringify({
      text,
      style: withTextRuns(style, flat, '<Dialogue>'),
      ...(maxWidth !== undefined ? { maxWidth } : {}),
      fonts,
    })
    : null;
  // Measured when `render` first reads `metrics`, so a render that never
  // does needs no text shaper (inspect.mjs has none).
  const measure = React.useMemo(() => {
    let metrics: TextMetrics | undefined;
    return () =>
      (metrics ??= synchronousMeasurer('subtitle.render')(JSON.parse(key!) as MeasureTextRequest));
  }, [key]);
  const changedStyle = themedStyle !== subtitle?.style ? themedStyle : undefined;
  if (!render || !character || key === null || !runtime) {
    return [null, changedStyle];
  }
  const localFrame = Math.round(secondsFromTime(runtime.time) * runtime.fps);
  const { style: _style, maxWidth: _maxWidth, render: _render, id: _id, emphasisStyles: _emphasisStyles, ...placement } = subtitle;
  const name = character.name ?? character.id;
  const info: SubtitleRenderProps = {
    text,
    content: shown.content,
    lineText: shown.lineText ?? text,
    character: { id: character.id, name, displayName: character.displayName ?? name },
    get metrics() {
      return measure();
    },
    style,
    ...(maxWidth !== undefined ? { maxWidth } : {}),
    held,
    page: shown.page,
    emphasis: shown.emphasis,
    frame: localFrame + (run?.offset ?? 0),
    durationInFrames: run?.durationInFrames ?? runtime.durationInFrames,
  };
  return [React.createElement(Group, placement, React.createElement(SubtitleRenderer, { render, info })), changedStyle];
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
