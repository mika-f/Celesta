import * as React from 'react';
import type { ReactNode } from 'react';

import {
  CompositionRuntimeContext,
  FreezeFrameContext,
  RootRuntimeContext,
  resolveTextLanguage,
} from './hooks';
import type { Animatable, BlendMode, Paint, TextStyle, LayerShadow, LayerGlow } from './scene';
import type { ShaderEffect } from './shader';
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
  /**
   * How the object's pixels combine with what is drawn beneath it, like CSS
   * `mix-blend-mode`. Defaults to `'normal'`. On a `Group` any other mode
   * isolates the group: its children composite together first, and the
   * result blends with the backdrop as one layer.
   */
  blendMode?: BlendMode;
  /** Gaussian blur radius in output pixels. */
  blur?: number;
  /** A colored shadow behind the composited layer. */
  shadow?: LayerShadow;
  /** A centered colored halo behind the composited layer. */
  glow?: LayerGlow;
  /**
   * A custom WGSL filter from `@celesta/shader`, applied to the composited
   * layer before `blur`, `shadow`, and `glow`.
   */
  shader?: ShaderEffect;
}

export interface CompositionProps {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  /** Default text language for font fallback; overridden by `style.lang`. */
  lang?: string;
  children?: ReactNode;
}

/** A rectangle in a group's own coordinate space, in pixels. */
export interface ClipRect {
  /** Left edge, in the group's coordinates. Defaults to 0. */
  x?: number;
  /** Top edge, in the group's coordinates. Defaults to 0. */
  y?: number;
  width: number;
  height: number;
  /** Corner radius, limited to half the shorter side. Defaults to 0. */
  cornerRadius?: number;
}

export interface GroupProps extends CommonProps {
  /** Default text language within this group; inherited when omitted. */
  lang?: string;
  /**
   * Draws the children only inside this rectangle, in the group's own
   * coordinate space (the space the children's `x`/`y` are given in), so it
   * moves, scales, and rotates with the group. The edge is anti-aliased and
   * clips nested inside one another intersect. A rectangle with no area
   * hides the children.
   */
  clip?: ClipRect;
  children?: ReactNode;
}

export interface ImageProps extends CommonProps {
  /** Display width in pixels. Alone, preserves the source aspect ratio. */
  width?: number;
  /** Display height in pixels. Alone, preserves the source aspect ratio. */
  height?: number;
  /** Center inside both dimensions; contain letterboxes, cover crops. */
  fit?: 'contain' | 'cover';
  src: AssetInput;
  name?: string;
}

export interface TextProps extends Omit<CommonProps, 'anchorY'> {
  /**
   * Vertical pivot as for `CommonProps.anchorY`, measured on the visible
   * letters of single-line text. `'baseline'` pivots on the first line's
   * baseline instead, so `Text` layers at the same `y` line up whatever their
   * glyphs or font sizes.
   */
  anchorY?: number | 'baseline';
  /**
   * Strings, numbers, and `<Span>` elements (arrays and fragments of
   * those). `null`, `undefined`, and booleans draw nothing.
   */
  children: ReactNode;
  style?: TextStyle;
  /** Text language, overriding ancestors; an explicit `style.lang` takes priority. */
  lang?: string;
  /**
   * Wraps lines to fit this width, breaking where Unicode line breaking
   * (UAX #14) allows: at spaces in Latin text, and between most characters
   * in Japanese and Chinese. A word wider than `maxWidth` is not split; the
   * part past `maxWidth` is cut off.
   */
  maxWidth?: number;
}

export interface RectProps extends CommonProps {
  width: number;
  height: number;
  /**
   * Hex fill color (`"#RRGGBB"` or `"#RRGGBBAA"`), or a `Paint` for a linear or
   * radial gradient in the rect's local pixels (origin at its top-left).
   * Omit for no fill.
   */
  fill?: string | Paint;
  /** Stroke color or `Paint`; has no visible effect without `strokeWidth`. */
  stroke?: string | Paint;
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
  /** `'image'`, `'video'`, `'audio'`, `'font'`, or one another package declares, such as `'character'`. */
  readonly kind: string;
  readonly src?: string;
}

export type AssetInput = string | AssetReference | React.RefObject<AssetReference>;

export interface AssetsProps {
  children?: ReactNode;
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
  const runtime = React.useContext(CompositionRuntimeContext);
  const lang = resolveTextLanguage(props.lang);
  return React.createElement(
    CompositionRuntimeContext.Provider,
    { value: runtime ? { ...runtime, lang } : null },
    React.createElement('composition', props),
  );
}

export function Group(props: GroupProps): ReturnType<typeof React.createElement> {
  const runtime = React.useContext(CompositionRuntimeContext);
  const lang = resolveTextLanguage(props.lang, runtime?.lang);
  const element = React.createElement('group', props);
  return runtime && props.lang !== undefined
    ? React.createElement(CompositionRuntimeContext.Provider, { value: { ...runtime, lang } }, element)
    : element;
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
  /** Default text language within this sequence; inherited when omitted. */
  lang?: string;
  /**
   * Frame this sequence starts at, in the enclosing timeline's own frame
   * numbering (the root composition's clock, or its parent `<Sequence>`'s
   * shifted clock when nested). Defaults to 0.
   */
  from?: number;
  /**
   * Length in frames. Children are only mounted — and only contribute audio —
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
 * sequence does. Children are unmounted outside the window, so their code
 * does not run and they contribute neither layers nor audio. Sequences work
 * under conditionals, `.map()`, and hooks like any other component.
 */
export function Sequence(props: SequenceProps): ReturnType<typeof React.createElement> {
  // Gate children before React evaluates them, using the same local window
  // as render.ts's walker. The walker still clips layers and audio against
  // the enclosing windows.
  const context = React.useContext(CompositionRuntimeContext);
  if (!context) {
    throw new Error('<Sequence> must be called from within a Celesta <Composition>');
  }
  const lang = resolveTextLanguage(props.lang, context.lang);
  const fps = context.fps;
  const localSeconds = secondsFromTime(context.time);
  const from = typeof props.from === 'number' && Number.isFinite(props.from) ? props.from : 0;
  const durationInFrames = props.durationInFrames;
  const hasDuration = typeof durationInFrames === 'number' && Number.isFinite(durationInFrames);
  const duration =
    hasDuration
      ? Math.max(0, durationInFrames)
      : Math.max(0, context.durationInFrames - from);
  const endSeconds = hasDuration ? (from + duration) / fps : context.durationInFrames / fps;
  if (localSeconds < from / fps || localSeconds >= endSeconds) {
    return React.createElement('sequence', props, null);
  }
  const shiftedTime = secondsToTime(localSeconds - from / fps);
  return React.createElement(
    CompositionRuntimeContext.Provider,
    {
      value: {
        ...context,
        lang,
        time: shiftedTime,
        durationInFrames: duration,
      },
    },
    React.createElement('sequence', props),
  );
}

export interface FreezeFrameProps extends CommonProps {
  /**
   * Composition frame the children are drawn at, counted on the root
   * composition's clock whatever `<Sequence>`s enclose this element.
   */
  frame: number;
  children?: ReactNode;
}

/**
 * Draws its children as the composition looked at `frame`: directly inside,
 * `useCurrentFrame()` is `frame` on the root clock, rounded like any time (a
 * `<Sequence>` within still counts from its own start), `useVideoConfig()`
 * reports the composition's duration, and `<Sequence>`s, `<Video>`, lip
 * sync, and blinking all follow that frame. A constant `frame` holds still; a moving one
 * replays the composition at that offset. Frozen children are always silent.
 * `<CharacterView>`s inside never attach their ref, which stays with the live
 * view, and `<Dialogue>`s inside drive only views drawn inside the same
 * `<FreezeFrame>`. Authored `id`s inside are prefixed with this layer's id.
 * Project layers (`<ProjectTimeline />`, `<ProjectTrack />`) are evaluated at
 * the current frame only, so they throw inside.
 */
export function FreezeFrame(props: FreezeFrameProps): ReturnType<typeof React.createElement> {
  const context = React.useContext(CompositionRuntimeContext);
  const root = React.useContext(RootRuntimeContext);
  const [scope] = React.useState(() => ({}));
  if (!context || !root) {
    throw new Error('<FreezeFrame> must be called from within a Celesta <Composition>');
  }
  if (typeof props.frame !== 'number' || !Number.isFinite(props.frame)) {
    throw new Error('<FreezeFrame> requires a finite `frame`');
  }
  return React.createElement(
    CompositionRuntimeContext.Provider,
    { value: { ...root, lang: context.lang, time: secondsToTime(props.frame / root.fps) } },
    React.createElement(FreezeFrameContext.Provider, { value: scope }, React.createElement('freeze-frame', props)),
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
