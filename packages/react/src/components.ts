import * as React from 'react';
import type { ReactNode } from 'react';

import { CompositionRuntimeContext } from './hooks';
import { useOptionalLipSync } from './lipsync';
import type { LipSyncTrack } from './lipsync';
import type { Animatable, TextStyle } from './scene';
import { secondsFromTime, secondsToTime } from './time';

// Unlike the pre-reconciler tree walker, these are real function components:
// each returns a host element (a lowercase intrinsic type the reconciler in
// reconciler.ts understands), so ordinary React composition — conditionals,
// `.map()`, context, hooks — works through them exactly as it would for any
// other component.

export interface CommonProps {
  id?: string;
  /**
   * Position of the object's top-left corner in its parent's coordinate
   * space, in pixels. Defaults to 0. (Set `anchorX`/`anchorY` to 0.5 to make
   * `x`/`y` address the centre instead.)
   */
  x?: number;
  /** Top-left corner Y, in pixels. Defaults to 0. See `x`. */
  y?: number;
  scale?: number;
  scaleX?: number;
  scaleY?: number;
  rotation?: number;
  /**
   * Normalized pivot the object is positioned by and that `scale`/`rotation`
   * turn about: 0 is the left edge, 1 the right, 0.5 the centre. Defaults to
   * 0, so `x`/`y` place the top-left corner. Use 0.5 to position and
   * rotate/scale about the centre.
   */
  anchorX?: number;
  /** Vertical pivot: 0 top, 1 bottom, 0.5 centre. Defaults to 0. See `anchorX`. */
  anchorY?: number;
  opacity?: number;
}

export interface CompositionProps {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  children?: ReactNode;
}

export interface GroupProps extends CommonProps {
  children?: ReactNode;
}

export interface ImageProps extends CommonProps {
  src: AssetInput;
  name?: string;
}

export interface TextProps extends CommonProps {
  children: ReactNode;
  style?: TextStyle;
  maxWidth?: number;
}

export interface RectProps extends CommonProps {
  width: number;
  height: number;
  /** Hex fill color (`"#RRGGBB"` or `"#RRGGBBAA"`). Omit for no fill. */
  fill?: string;
  /** Hex stroke color; has no visible effect without `strokeWidth`. */
  stroke?: string;
  strokeWidth?: number;
  cornerRadius?: number;
}

export interface VideoProps extends CommonProps {
  src: AssetInput;
  name?: string;
  /**
   * Seconds into the source file playback starts from, at the sequence's own
   * frame 0 — the counterpart of a project timeline clip's trim-in point.
   * Defaults to 0.
   */
  startFrom?: number;
  /**
   * Speed the source plays back at relative to the enclosing sequence's own
   * clock (2 plays twice as fast, 0.5 half as fast). Defaults to 1. A React
   * `<Video>` has no per-frame automation the way a project timeline clip's
   * `Animatable<f64>` playback rate does — it is a single static value for
   * the whole clip.
   */
  playbackRate?: number;
}

export interface AssetReference {
  readonly id: string;
  readonly kind: 'character' | 'image' | 'video' | 'audio' | 'font';
  readonly src?: string;
  readonly portrait?: CharacterPortrait;
  readonly subtitle?: CharacterSubtitle;
}

export type AssetInput = string | AssetReference | React.RefObject<AssetReference>;

export interface AssetsProps {
  children?: ReactNode;
}

export interface CharacterProps {
  name: string;
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
  lipSync?: PsdCharacterLipSync;
}

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
}

export interface CharacterSubtitle extends CommonProps {
  style?: TextStyle;
  maxWidth?: number;
}

export interface DialogueProps extends CommonProps {
  character: AssetInput;
  children: ReactNode;
  expression?: string;
  mouth?: CharacterViewProps['mouth'];
  lipSync?: LipSyncTrack;
  audio?: AssetInput;
  startFrom?: number;
  playbackRate?: AnimatedNumber;
  volume?: AnimatedNumber;
  muted?: boolean;
}

export interface FontProps {
  src: AssetInput;
  name?: string;
}

const AssetsContext = React.createContext(false);

export function Assets(props: AssetsProps): ReturnType<typeof React.createElement> {
  return React.createElement(
    AssetsContext.Provider,
    { value: true },
    React.createElement('assets', props),
  );
}

export const Character = React.forwardRef<AssetReference, CharacterProps>(function Character(
  props,
  ref,
) {
  const reference = React.useMemo<AssetReference>(
    () => ({
      id: props.id ?? props.name,
      kind: 'character',
      portrait: props.portrait,
      subtitle: props.subtitle,
    }),
    [props.id, props.name, props.portrait, props.subtitle],
  );
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement('asset-character', props);
});

function assetReference(
  kind: AssetReference['kind'],
  src: AssetInput,
  name?: string,
): AssetReference {
  // `src` may be a plain path, an `AssetReference`, or a ref object still
  // holding its initial `undefined`. Guard the `in` check so a missing or
  // nullish `src` falls through to the error below instead of throwing a
  // bare `TypeError: Cannot use 'in' operator to search for 'current' in
  // undefined` — this runs during `mount()`'s first render pass, so that
  // raw error would otherwise surface as "could not load the React
  // composition".
  const value =
    typeof src === 'string' ? src : src != null && 'current' in src ? src.current : src;
  if (!value) {
    throw new Error(
      `<${kind[0].toUpperCase()}${kind.slice(1)}> requires a non-empty \`src\`, but it was ${
        src == null ? String(src) : 'an unassigned ref'
      }`,
    );
  }
  return {
    id: name ?? (typeof value === 'string' ? value : value.id),
    kind,
    src: typeof value === 'string' ? value : value.src,
  };
}

function assignAssetRef(ref: React.ForwardedRef<AssetReference>, value: AssetReference): void {
  if (ref && typeof ref === 'object') {
    ref.current = value;
  }
}

export function Composition(props: CompositionProps): ReturnType<typeof React.createElement> {
  return React.createElement('composition', props);
}

export function Group(props: GroupProps): ReturnType<typeof React.createElement> {
  return React.createElement('group', props);
}

export function CharacterView(props: CharacterViewProps): ReturnType<typeof React.createElement> {
  const { lipSync, ...rest } = props;
  const tracked = useOptionalLipSync(lipSync);
  const mouth = rest.mouth ?? tracked;
  return React.createElement('character-view', { ...rest, mouth });
}

/** Renders a character portrait and its configured subtitle as one layer. */
export function Dialogue(props: DialogueProps): ReturnType<typeof React.createElement> {
  const { lipSync, ...rest } = props;
  const tracked = useOptionalLipSync(lipSync);
  return React.createElement('dialogue', { ...rest, mouth: rest.mouth ?? tracked });
}

export const Image = React.forwardRef<AssetReference, ImageProps>(function Image(props, ref) {
  const inAssets = React.useContext(AssetsContext);
  const reference = React.useMemo(() => assetReference('image', props.src, props.name), [props.src, props.name]);
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement(inAssets ? 'asset-image' : 'image', props);
});

export function Text(props: TextProps): ReturnType<typeof React.createElement> {
  return React.createElement('text', props);
}

/** A flat-shaded rectangle, optionally rounded and/or stroked. */
export function Rect(props: RectProps): ReturnType<typeof React.createElement> {
  return React.createElement('rect', props);
}

export const Video = React.forwardRef<AssetReference, VideoProps>(function Video(props, ref) {
  const inAssets = React.useContext(AssetsContext);
  const reference = React.useMemo(
    () => assetReference('video', props.src, props.name),
    [props.name, props.src],
  );
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement(inAssets ? 'asset-video' : 'video', props);
});

export const Font = React.forwardRef<AssetReference, FontProps>(function Font(props, ref) {
  const reference = React.useMemo(() => assetReference('font', props.src, props.name), [props.src, props.name]);
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement('asset-font', props);
});

export interface SequenceProps extends CommonProps {
  /**
   * Frame this sequence starts at, in the enclosing timeline's own frame
   * numbering (the root composition's clock, or its parent `<Sequence>`'s
   * shifted clock when nested). Defaults to 0.
   */
  from?: number;
  /**
   * Length in frames. Children are only visible — and only contribute audio —
   * inside `[from, from + durationInFrames)`; defaults to running until the
   * end of the enclosing window.
   */
  durationInFrames?: number;
  children?: ReactNode;
}

/**
 * Places its children at an offset of the composition's timeline. Inside a
 * `<Sequence>`, `useCurrentFrame()`/`useCurrentTime()` report the shifted
 * local clock (`frame - from`), and `<Video>`/`<Audio>` play synced to that
 * same local clock, so media placed inside a sequence starts when the
 * sequence does. Children outside the window render no layers and collect no
 * audio; the range itself is evaluated by the renderer per requested frame,
 * so sequences work under conditionals, `.map()`, and hooks like any other
 * component.
 */
export function Sequence(props: SequenceProps): ReturnType<typeof React.createElement> {
  // The host element ('sequence') carries only static props; shifting the
  // clock for descendant components must happen here, through context, while
  // they render. Whether the sequence is active at a given composition time
  // is decided later, by render.ts's walker, which sees every 'sequence'
  // node regardless of the current time.
  const context = React.useContext(CompositionRuntimeContext);
  if (!context) {
    throw new Error('<Sequence> must be called from within a Mikan <Composition>');
  }
  const fps = context.fps;
  const localSeconds = secondsFromTime(context.time);
  const from = typeof props.from === 'number' && Number.isFinite(props.from) ? props.from : 0;
  const shiftedTime = secondsToTime(localSeconds - from / fps);
  const duration =
    typeof props.durationInFrames === 'number'
      ? Math.max(0, props.durationInFrames)
      : Math.max(0, context.durationInFrames - from);
  return React.createElement(
    CompositionRuntimeContext.Provider,
    {
      value: {
        ...context,
        time: shiftedTime,
        durationInFrames: duration,
      },
    },
    React.createElement('sequence', props),
  );
}

export type AnimatedNumber = Animatable<number>;

export interface AudioProps extends CommonProps {
  src: AssetInput;
  /**
   * Seconds into the source file playback starts from, at the sequence's own
   * frame 0. Defaults to 0.
   */
  startFrom?: number;
  /**
   * Speed the source plays back at relative to the enclosing sequence's own
   * clock. Defaults to 1. Either a plain number or a generated
   * `KeyframeAnimation` over seconds-into-the-sequence times, matching the
   * project format's `Animatable<f64>` playback rate.
   */
  playbackRate?: AnimatedNumber;
  /**
   * Linear volume multiplier, 0 silent, 1 unchanged. Defaults to 1. Either a
   * plain number or a generated `KeyframeAnimation` over
   * seconds-into-the-sequence times, matching the project format's
   * `Animatable<f64>` volume automation.
   */
  volume?: AnimatedNumber;
  /** Defaults to false. */
  muted?: boolean;
}

/**
 * Declares one audio clip playing synced to the enclosing timeline's own
 * clock — bare, it spans the whole composition; inside one or more
 * `<Sequence>`s it is bounded by them and starts when the innermost sequence
 * does. Audio declarations are gathered from the rendered frame tree (the
 * same walk that produces layers), so an `<Audio>` behind an ordinary React
 * conditional or hook contributes sound on exactly the frames where it
 * renders, with its full audible range derived from any enclosing sequences.
 */
export const Audio = React.forwardRef<AssetReference, AudioProps>(function Audio(props, ref) {
  const inAssets = React.useContext(AssetsContext);
  const reference = React.useMemo(() => assetReference('audio', props.src), [props.src]);
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement(inAssets ? 'asset-audio' : 'audio', props);
});
