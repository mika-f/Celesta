import * as React from 'react';
import type { ReactNode } from 'react';
import { googleFont, type GoogleFontOptions } from './google-font';

import {
  CompositionRuntimeContext,
  CompositionSelectionContext,
  FreezeFrameContext,
  RootRuntimeContext,
  resolveTextLanguage,
} from './hooks';
import type { Animatable, BlendMode, LineCap, Paint, PathCommand, TextStyle, LayerShadow, LayerGlow } from './scene';
import type { LayerTint, LayerVignette, ShaderEffect } from './shader';
import { endsWithComposition, secondsFromTime, secondsToTime } from './time';

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
  /**
   * 0–1, multiplied down through groups. A `Group` below 1 is isolated like
   * one with a `blendMode`, as under CSS `opacity`: its children composite
   * together first and fade as one layer, so overlapping children do not
   * show through one another.
   */
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
   * Filters from `@celesta/shader` (custom WGSL shaders from
   * `defineShader()`, or built-in color filters such as `brightness(1.2)`),
   * applied to the composited layer before the color filter props, `blur`,
   * `shadow`, and `glow`. An array runs them in order, each filtering the
   * result of the one before it.
   */
  shader?: ShaderEffect | readonly ShaderEffect[];
  /*
   * The color filters below apply after `shader` and before `blur`,
   * `shadow`, and `glow`, in the order listed here. Each works on the
   * straight sRGB color, keeps alpha, and clamps its result to 0–1. For
   * another order, pass them to `shader` as an array instead.
   */
  /** Multiplies the color: 1 keeps it, 0 is black, above 1 brightens. */
  brightness?: number;
  /** Scales the distance from mid-gray: 1 keeps it, 0 is mid-gray, above 1 adds contrast. */
  contrast?: number;
  /** 1 keeps the color, 0 is grayscale, above 1 saturates. */
  saturation?: number;
  /** Rotates the hue, in degrees. */
  hue?: number;
  /** 0 keeps the color, 1 is grayscale. */
  grayscale?: number;
  /** 0 keeps the color, 1 inverts it. */
  invert?: number;
  /** Lays a color over the pixels, keeping their alpha. */
  tint?: LayerTint;
  /** Darkens toward the edges of the content box. A number is the amount, 0–1. */
  vignette?: number | LayerVignette;
}

export interface CompositionProps {
  /** Unique within the entry. Required when registering multiple compositions. */
  id?: string;
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

export interface MaskProps {
  /** What decides visibility: the mask's alpha (default) or its luminance. */
  mode?: 'alpha' | 'luminance';
  /** Shows the group where the mask is not drawn instead. Defaults to false. */
  invert?: boolean;
  children?: ReactNode;
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
  /**
   * Shows the children only where this is drawn: by its alpha, or as a
   * `<Mask>` element sets. Drawn in the group's own coordinate space and
   * never shown itself. `null`, `undefined`, and booleans mean no mask; an
   * empty `<Mask />` hides the group (shows it whole when inverted).
   * Masking isolates the group, like a blend mode other than `'normal'`.
   */
  mask?: ReactNode;
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
  /**
   * An animated GIF, APNG, or WebP plays synced to the enclosing sequence's
   * own clock from its frame 0, showing each frame for its own delay. These
   * props are ignored for a still image, and inside `<Static>`, which draws
   * the first frame.
   *
   * Seconds into the animation playback starts from. Defaults to 0.
   */
  startFrom?: number;
  /** Speed relative to the enclosing sequence's clock. Defaults to 1. */
  playbackRate?: number;
  /** Starts over after the last frame instead of holding it. Defaults to true. */
  loop?: boolean;
}

export interface TextProps extends Omit<CommonProps, 'anchorY'> {
  /**
   * Vertical pivot as for `CommonProps.anchorY`, measured on the visible
   * letters of single-line text and on the line boxes of text with several
   * lines (broken with `\n` or wrapped at `maxWidth`), as `measureText`
   * reports them. `'baseline'` pivots on the first line's
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
  /**
   * Breaks the stroke into dashes: lengths of the dashes and the gaps
   * between them, in turn, in pixels, from the end of the top-left corner
   * clockwise, like SVG's `stroke-dasharray`. An odd count repeats once
   * more. With `cap` `'round'`, `[0, gap]` draws dots. A dashed rect is
   * drawn as a path, its fill as a rect beneath it, so a translucent dash
   * shows the fill through it.
   */
  dash?: readonly number[];
  /** How far into `dash` the stroke starts; animate it to make the dashes flow. */
  dashOffset?: number;
  /** How each dash ends: `'butt'` (the default), `'round'`, or `'square'`. Only matters with `dash`. */
  cap?: LineCap;
}

export interface StaticProps {
  /**
   * The subtree stays the same until one of these changes (compared with
   * `Object.is`, element by element). Without `deps` it stays the same for
   * as long as it is mounted.
   */
  deps?: readonly unknown[];
  children?: ReactNode;
}

function sameDeps(previous: StaticProps, next: StaticProps): boolean {
  const [a, b] = [previous.deps, next.deps];
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  return a.every((value, index) => Object.is(value, b[index]));
}

/**
 * A subtree that is the same from frame to frame: React renders it only when
 * `deps` change, and its layers are walked, sent to Rust, and parsed once
 * and reused. A component inside that reads the frame still works, but is
 * walked and sent again on every frame it changes. It cannot contain
 * anything whose layers depend on the time: `<Video>`, `<Audio>`,
 * `<Sequence>`, `<FreezeFrame>`, or registered host elements.
 */
export const Static = React.memo(function Static({ children }: StaticProps) {
  return React.createElement('static', null, children);
}, sameDeps);

/** One value per particle: a `Float32Array` (fastest) or an array of numbers. */
export type InstanceColumn = Float32Array | ArrayLike<number>;

export interface InstancesProps
  extends Omit<CommonProps, 'x' | 'y' | 'rotation' | 'anchorX' | 'anchorY'> {
  /** Number of particles; every column holds this many values. */
  count: number;
  /** Particle centres, in the layer's own coordinates. */
  x: InstanceColumn;
  y: InstanceColumn;
  /**
   * A number for every particle, or one value per particle. Required for a
   * rect or circle; an image defaults to its natural size.
   */
  width?: number | InstanceColumn;
  height?: number | InstanceColumn;
  /** Degrees, clockwise, about each particle's centre. Defaults to 0. */
  rotation?: InstanceColumn;
  /**
   * Straight-alpha RGBA in 0-255, four values per particle. A rect or circle
   * is filled with it; an image's texels are multiplied by it.
   */
  color?: Uint8Array | ArrayLike<number>;
  /** Hex colour of every particle when `color` is absent. Defaults to white. */
  fill?: string;
  /** Defaults to `'rect'`. Ignored when `src` is given. */
  shape?: 'rect' | 'circle';
  /** Corner radius of a rect, limited to half its shorter side. */
  cornerRadius?: number;
  /** Draws this image for every particle instead of a shape. */
  src?: AssetInput;
}

/**
 * Many particles of one shape (a rect, a circle, or an image) as one layer,
 * drawn in order: particle `i` is centred on `(x[i], y[i])`. Tens of
 * thousands animate every frame at the cost of a few layers. `x`, `y`, and
 * `rotation` are always per-particle; wrap the layer in a `<Group>` to move
 * or rotate the particles as a whole. A `blendMode` blends the composited
 * particles as one layer, as on a `<Group>`.
 */
export function Instances(props: InstancesProps): ReturnType<typeof React.createElement> {
  return React.createElement('instances', props);
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
   * the whole clip. It applies to the soundtrack too.
   */
  playbackRate?: number;
  /**
   * Linear volume multiplier for the video's soundtrack, 0 silent, 1
   * unchanged. Defaults to 1. Either a plain number or a generated
   * `KeyframeAnimation` over seconds-into-the-sequence times, as on `<Audio>`.
   */
  volume?: AnimatedNumber;
  /** Silences the video's soundtrack. Defaults to false. */
  muted?: boolean;
  /**
   * Past the end of the file, starts the picture and soundtrack over from
   * the file's beginning instead of holding the last frame in silence.
   * Defaults to false.
   */
  loop?: boolean;
}

export interface AssetReference {
  readonly id: string;
  /** `'image'`, `'video'`, `'audio'`, `'font'`, or one another package declares, such as `'character'`. */
  readonly kind: string;
  readonly src?: string;
}

export type AssetInput = string | AssetReference | React.RefObject<AssetReference | null>;

export interface AssetsProps {
  children?: ReactNode;
}

export type FontProps = {
  name?: string;
} & ({ src: AssetInput; google?: never; weights?: never; subsets?: never }
  | ({ src?: never; google: string } & GoogleFontOptions));

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

export function Composition(props: CompositionProps): ReturnType<typeof React.createElement> | null {
  const runtime = React.useContext(CompositionRuntimeContext);
  const selection = React.useContext(CompositionSelectionContext);
  const lang = resolveTextLanguage(props.lang);
  if (selection === null) return React.createElement('composition', { ...props, children: undefined });
  if (selection !== undefined && selection !== (props.id ?? 'default')) return null;
  return React.createElement(
    CompositionRuntimeContext.Provider,
    { value: runtime ? { ...runtime, lang } : null },
    React.createElement('composition', props),
  );
}

export function Group({ mask, children, ...props }: GroupProps): ReturnType<typeof React.createElement> {
  const runtime = React.useContext(CompositionRuntimeContext);
  const lang = resolveTextLanguage(props.lang, runtime?.lang);
  // The mask goes after the children, always in the same slot, so turning it
  // on or off changes neither their tree-path ids nor their state. The
  // fragment keeps React from asking the children, often a static list, for
  // keys.
  const element = React.createElement(
    'group',
    props,
    React.createElement(React.Fragment, null, children),
    maskElement(mask),
  );
  return runtime && props.lang !== undefined
    ? React.createElement(CompositionRuntimeContext.Provider, { value: { ...runtime, lang } }, element)
    : element;
}

/** A `mask` prop as a `<Mask>` element, or null without a mask. */
function maskElement(mask: ReactNode): ReactNode {
  if (mask === null || mask === undefined || typeof mask === 'boolean') {
    return null;
  }
  return React.isValidElement(mask) && mask.type === Mask ? mask : React.createElement(Mask, null, mask);
}

/**
 * Decides, per pixel, how much of the enclosing `<Group>` shows: by alpha, or
 * by luminance with `mode="luminance"`. Put it directly in a `<Group>`, or
 * pass it as the group's `mask` prop. Its children are positioned in the
 * group's coordinate space and never shown themselves.
 */
export function Mask(props: MaskProps): ReturnType<typeof React.createElement> {
  return React.createElement('mask', props);
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
export function Rect({ dash, dashOffset, cap, ...props }: RectProps): ReturnType<typeof React.createElement> {
  const { width, height, fill, stroke, strokeWidth = 0, cornerRadius = 0, anchorX = 0, anchorY = 0, ...common } = props;
  if (dash === undefined || dash.length === 0 || stroke === undefined || !(strokeWidth > 0)) {
    return React.createElement('rect', props);
  }
  // A path has no anchor, so the group's origin is the anchor point; the
  // stroke is centred on the outline, so inset it by half its width.
  const left = -anchorX * width;
  const top = -anchorY * height;
  const drawn = Math.min(strokeWidth, Math.min(width, height) / 2);
  const inset = drawn / 2;
  const radius = Math.max(0, Math.min(cornerRadius, width / 2, height / 2) - inset);
  return React.createElement(
    'group',
    common,
    fill === undefined ? null : React.createElement('rect', { x: left, y: top, width, height, fill, cornerRadius }),
    React.createElement('path', {
      commands: roundedRectCommands(left + inset, top + inset, width - drawn, height - drawn, radius),
      stroke: offsetPaint(stroke, left, top),
      strokeWidth: drawn,
      cap,
      dash,
      dashOffset,
    }),
  );
}

/**
 * A `width` x `height` rectangle with corners rounded by `radius`, from the
 * end of its top-left corner clockwise, as SVG draws a `<rect>`; each corner
 * is two cubic arcs, as close to a circle as `<Ellipse>`'s.
 */
function roundedRectCommands(left: number, top: number, width: number, height: number, radius: number): PathCommand[] {
  const right = left + width;
  const bottom = top + height;
  const k = (4 / 3) * Math.tan(Math.PI / 16);
  const commands: PathCommand[] = [{ type: 'moveTo', x: left + radius, y: top }];
  // Each corner's centre, and the quarter turn its arc starts at (0 is +x,
  // clockwise in y-down pixels): top-right, bottom-right, bottom-left, top-left.
  const corners: [number, number, number, number, number][] = [
    [right - radius, top + radius, -2, right - radius, top],
    [right - radius, bottom - radius, 0, right, bottom - radius],
    [left + radius, bottom - radius, 2, left + radius, bottom],
    [left + radius, top + radius, 4, left, top + radius],
  ];
  for (const [cx, cy, start, x, y] of corners) {
    commands.push({ type: 'lineTo', x, y });
    if (radius === 0) {
      continue;
    }
    for (let i = start; i < start + 2; i += 1) {
      const [c0, s0] = [Math.cos((i * Math.PI) / 4), Math.sin((i * Math.PI) / 4)];
      const [c1, s1] = [Math.cos(((i + 1) * Math.PI) / 4), Math.sin(((i + 1) * Math.PI) / 4)];
      commands.push({
        type: 'cubicTo',
        x1: cx + radius * (c0 - k * s0),
        y1: cy + radius * (s0 + k * c0),
        x2: cx + radius * (c1 + k * s1),
        y2: cy + radius * (s1 - k * c1),
        x: cx + radius * c1,
        y: cy + radius * s1,
      });
    }
  }
  commands.push({ type: 'close' });
  return commands;
}

/** `paint` with its gradient points moved by `dx`/`dy`. */
function offsetPaint(paint: string | Paint, dx: number, dy: number): string | Paint {
  if (typeof paint === 'string' || paint.type === 'solid') {
    return paint;
  }
  const move = ({ x, y }: { x: number; y: number }) => ({ x: x + dx, y: y + dy });
  return paint.type === 'linear'
    ? { ...paint, start: move(paint.start), end: move(paint.end) }
    : { ...paint, center: move(paint.center) };
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
  if (props.google !== undefined && props.src !== undefined) {
    throw new Error('<Font> accepts either `src` or `google`, but not both.');
  }
  const src = props.google !== undefined ? googleFont(props.google, props) : props.src;
  const reference = React.useMemo(() => assetReference('font', src, props.name), [src, props.name]);
  assignAssetRef(ref, reference);
  React.useImperativeHandle(ref, () => reference, [reference]);
  return React.createElement('asset-font', { src, name: props.name });
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
  const root = React.useContext(RootRuntimeContext);
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
  // Past the composition's end (see `openEnd`), a window that ends with the
  // composition stays open; one that ends later keeps its own end.
  // render.ts's walker decides the same way.
  const openEnd =
    context.openEnd === true &&
    root !== null &&
    endsWithComposition(secondsFromTime(root.time) - localSeconds + endSeconds, root.durationInFrames / fps, fps);
  if (localSeconds < from / fps || (!openEnd && localSeconds >= endSeconds)) {
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
        openEnd,
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
    { value: { ...root, lang: context.lang, time: secondsToTime(props.frame / root.fps), openEnd: false } },
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
  /**
   * Past the end of the file, starts it over from its beginning instead of
   * falling silent. Defaults to false.
   */
  loop?: boolean;
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
