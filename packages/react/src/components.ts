import * as React from 'react';
import type { ReactNode } from 'react';
import type { ChapterSource } from './chapters';
import { googleFont, type GoogleFontOptions } from './google-font';

import {
  CompositionRuntimeContext,
  CompositionSelectionContext,
  FreezeFrameContext,
  RootRuntimeContext,
  resolveFontFallback,
  resolveTextLanguage,
} from './hooks';
import type { Animatable, BackfaceVisibility, BlendMode, FontFallbackMode, LineCap, Paint, PathCommand, TextStyle, LayerShadow, LayerGlow } from './scene';
import type {
  LayerChromaKey, LayerChromaticAberration, LayerHalftone, LayerNoise, LayerPixelate, LayerScanlines, LayerTint,
  LayerVignette, LayerWave, LayerZoomBlur, ShaderEffect,
} from './shader';
import { createdAtOverride } from './jsx-dev-runtime';
import { markPrimitive, markTransparent, markWindow } from './source-chain';
import { themeTextContent } from './rich-text';
import { type TextShorthand, ThemeContext, themedShapePaint, themedTextStyle } from './theme';
import { endsWithComposition, isBefore, secondsFromTime, secondsToTime } from './time';

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
   * The point `scale`, `rotation`, `skew`, `rotationX`, and `rotationY` turn
   * about, which is also the vanishing point of `perspective`: an offset in
   * the object's own pixels from its anchor (from a `Group`'s, `Path`'s, or
   * `Instances`'s own origin, as they have no box to anchor in). Defaults to
   * 0. Unlike the anchor it does not move the object: untransformed, it
   * stays where `x`/`y` put it. Turns a group about its centre without
   * nesting a second group:
   *
   * ```tsx
   * <Group originX={width / 2} originY={height / 2} rotationY={angle} perspective={2000}>
   *   {children}
   * </Group>
   * ```
   */
  originX?: number;
  /** Vertical pivot offset in pixels. Defaults to 0. See `originX`. */
  originY?: number;
  /**
   * Shear along the x axis, in degrees: points slide sideways in
   * proportion to their height. Defaults to 0. Like CSS `skewX()`. See
   * `rotationX` for how it composes with the other transform props.
   */
  skewX?: number;
  /** Shear along the y axis, in degrees. Defaults to 0. See `skewX`. */
  skewY?: number;
  /**
   * Tilt about the horizontal axis, in degrees, like CSS `rotateX()`: a
   * positive angle turns the top away from the viewer. Defaults to 0.
   * Without `perspective` the tilt is orthographic and only foreshortens.
   *
   * The transform composes as position · perspective · rotationX ·
   * rotationY · rotation · skew · scale, rightmost first, so the object
   * is scaled, then skewed, then turned, then tilted, then projected and
   * placed. A `Group` turns about its own origin, other layers about their
   * anchor, moved by `originX`/`originY`; that point is also the vanishing
   * point. Children flatten into the
   * parent's plane: a rotated `Group` carries its children as one flat
   * picture, with no depth sorting between them.
   */
  rotationX?: number;
  /**
   * Turn about the vertical axis, in degrees, like CSS `rotateY()`. Defaults
   * to 0. See `rotationX`.
   */
  rotationY?: number;
  /**
   * Distance from the viewer to the layer plane, in pixels, like CSS
   * `perspective()`: smaller values exaggerate the depth. Anything that is
   * not finite and positive means none, the default. A projective layer
   * (a 3D rotation with a perspective) is isolated: it is drawn as one flat
   * picture first and then projected.
   */
  perspective?: number;
  /**
   * Whether the object shows while its back faces the viewer, like CSS
   * `backface-visibility`. Defaults to `'visible'`; `'hidden'` skips the
   * object (and a `Group`'s children) once a 3D rotation turns it away.
   */
  backfaceVisibility?: BackfaceVisibility;
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
   * `defineShader()`, or built-in filters such as `brightness(1.2)` and
   * `pixelate(8)`), applied to the composited layer before the built-in
   * filter props, `blur`, `shadow`, and `glow`. An array runs them in
   * order, each filtering the result of the one before it.
   */
  shader?: ShaderEffect | readonly ShaderEffect[];
  /*
   * The built-in filters below apply after `shader` and before `blur`,
   * `shadow`, and `glow`, in the order listed here: the chroma key, the
   * distortions, the color filters, then the overlays. For another order,
   * pass the `@celesta/shader` functions of the same names to `shader` as
   * an array instead. Spatial values are output pixels; grids and `uv`-like
   * positions follow the layer's content box.
   */
  /**
   * Makes the colors near a key color transparent, as for green-screen
   * footage. A string is the key color.
   */
  chromaKey?: string | LayerChromaKey;
  /** Averages square cells of this many pixels. A number is the `size`. */
  pixelate?: number | LayerPixelate;
  /** Sways the pixels along a sine. */
  wave?: LayerWave;
  /** Blurs toward a centre, as a fast zoom would. A number is the `amount`, 0–1. */
  zoomBlur?: number | LayerZoomBlur;
  /** Moves red and blue apart. A number is the `offset` in pixels. */
  chromaticAberration?: number | LayerChromaticAberration;
  /*
   * The color filters work on the straight sRGB color, keep alpha, and
   * clamp their result to 0–1.
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
  /**
   * Dots of each cell's color whose area follows its brightness, with
   * transparency between them. A number is the grid's `size` in pixels.
   */
  halftone?: number | LayerHalftone;
  /**
   * Grain over the color, the same on any render of the same frame. A
   * number is the `amount`, 0–1; it changes every frame unless given a `seed`.
   */
  noise?: number | LayerNoise;
  /** Darkened horizontal bands. A number is the `amount`, 0–1. */
  scanlines?: number | LayerScanlines;
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
  /**
   * Where characters a text's `fontFamily` lacks are drawn from, for text
   * within: `'loaded'` tries fonts loaded with `<Font>` first, in declaration
   * order. Defaults to `'system'`, installed fonts only; `style.fontFallback` takes priority.
   */
  fontFallback?: FontFallbackMode;
  /**
   * Named spans of this composition's timeline, which the editor preview
   * lists and seeks to: a `defineScenes` list, a dialogue plan, or spans of
   * `{ id, name?, from, durationInFrames }` in this composition's frames
   * (see `listChapters`). Static like the size and clock. Chapters are
   * ordered by their start and cut at the end; one starting at or after the
   * end, a duplicate `id`, or a bad frame fails the mount.
   */
  chapters?: ChapterSource;
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
   * Where characters a text's `fontFamily` lacks are drawn from, for text
   * within: `'loaded'` tries fonts loaded with `<Font>` first, in declaration
   * order. Inherited when omitted; `style.fontFallback` takes priority.
   */
  fontFallback?: FontFallbackMode;
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

/**
 * `fontFamily`, `fontSize`, `fontWeight`, and `color` are shorthand for
 * `style`; where both set the same thing, `style` wins.
 */
export interface TextProps extends Omit<CommonProps, 'anchorY'>, TextShorthand {
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
   * Hex fill color (`"#RRGGBB"` or `"#RRGGBBAA"`) or `<Theme>` color name,
   * or a `Paint` for a linear or radial gradient in the rect's local pixels
   * (origin at its top-left), whose colors may be theme names too. Omit for
   * no fill.
   */
  fill?: string | Paint;
  /** Stroke color, as for `fill`, or `Paint`; has no visible effect without `strokeWidth`. */
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
}, (previous, next) =>
  // The editor's drag preview creates elements with its prop overrides (see
  // `PropOverride`); children written here at an override's position render
  // on every frame of the drag, others are kept as without one. The cached
  // layers stay valid: they are walked again only when a host node below
  // changed.
  sameDeps(previous, next) && !createdAtOverride(next.children));

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
  /** The group the soundtrack belongs to, for other clips' `duck.under`. */
  duckGroup?: string;
  /** Turns the soundtrack down while clips of other groups play. */
  duck?: AudioDuck;
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
} & ({ src: AssetInput; google?: never; weights?: never; subsets?: never; text?: never }
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
  const fontFallback = resolveFontFallback(props.fontFallback);
  if (selection === null) return React.createElement('composition', { ...props, children: undefined });
  if (selection !== undefined && selection !== (props.id ?? 'default')) return null;
  return React.createElement(
    CompositionRuntimeContext.Provider,
    { value: runtime ? { ...runtime, lang, fontFallback } : null },
    React.createElement('composition', props),
  );
}

export function Group({ mask, children, ...props }: GroupProps): ReturnType<typeof React.createElement> {
  const runtime = React.useContext(CompositionRuntimeContext);
  const lang = resolveTextLanguage(props.lang, runtime?.lang);
  const fontFallback = resolveFontFallback(props.fontFallback, runtime?.fontFallback);
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
  return runtime && (props.lang !== undefined || props.fontFallback !== undefined)
    ? React.createElement(CompositionRuntimeContext.Provider, { value: { ...runtime, lang, fontFallback } }, element)
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

export function Text({ fontFamily, fontSize, fontWeight, color, ...props }: TextProps): ReturnType<typeof React.createElement> {
  const theme = React.useContext(ThemeContext);
  const style = themedTextStyle(props.style, { fontFamily, fontSize, fontWeight, color }, theme, '<Text>');
  const children = themeTextContent(props.children, theme);
  return React.createElement('text', style === props.style && children === props.children
    ? props
    : { ...props, style, children });
}

/** A flat-shaded rectangle, optionally rounded and/or stroked. */
export function Rect({ dash, dashOffset, cap, ...authored }: RectProps): ReturnType<typeof React.createElement> {
  const theme = React.useContext(ThemeContext);
  const themedFill = themedShapePaint(authored.fill, theme, '<Rect>', 'fill');
  const themedStroke = themedShapePaint(authored.stroke, theme, '<Rect>', 'stroke');
  const props = themedFill === authored.fill && themedStroke === authored.stroke
    ? authored
    : { ...authored, fill: themedFill, stroke: themedStroke };
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
   * Where characters a text's `fontFamily` lacks are drawn from, for text
   * within: `'loaded'` tries fonts loaded with `<Font>` first, in declaration
   * order. Inherited when omitted; `style.fontFallback` takes priority.
   */
  fontFallback?: FontFallbackMode;
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
  const fontFallback = resolveFontFallback(props.fontFallback, context.fontFallback);
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
  if (isBefore(localSeconds, from / fps) || (!openEnd && !isBefore(localSeconds, endSeconds))) {
    return React.createElement('sequence', props, null);
  }
  const shiftedTime = secondsToTime(localSeconds - from / fps);
  return React.createElement(
    CompositionRuntimeContext.Provider,
    {
      value: {
        ...context,
        lang,
        fontFallback,
        time: shiftedTime,
        durationInFrames: duration,
        openEnd,
      },
    },
    React.createElement('sequence', props),
  );
}

interface FreezeFrameLink {
  frame: number;
  /** `frame` as the rounded root time, in microseconds, which cycles compare. */
  time: number;
  id?: string;
  /** The children's element types, which tell a replay of this `<FreezeFrame>` from another one. */
  children: unknown[];
}

function sameChildren(a: readonly unknown[], b: readonly unknown[]): boolean {
  return a.length === b.length && a.every((type, index) => type === b[index]);
}

/**
 * How many `<FreezeFrame>`s may enclose one another. Each level replays the
 * film at another frame, so a film that freezes a moving frame of itself
 * (`frame={frame + 1}`) never repeats one, yet never ends either.
 */
const MAX_FREEZE_FRAME_DEPTH = 16;

/** The `<FreezeFrame>`s enclosing this point, outermost first. */
const FreezeFrameChainContext = React.createContext<readonly FreezeFrameLink[]>([]);

function describeFreezeChain(chain: readonly FreezeFrameLink[]): string {
  return chain
    .map((link) => (link.id === undefined ? `frame ${link.frame}` : `frame ${link.frame} (id "${link.id}")`))
    .join(' > ');
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
 * `<FreezeFrame>`. A `frame` at or past the composition's end draws the
 * children as if the video went on: a `<Sequence>` that ends with the
 * composition stays open, one with its own end keeps it, and
 * `useVideoConfig()` still reports the composition's real duration, so a
 * short composition (a poster, say) can draw a frame of a longer video.
 * Authored `id`s inside are prefixed with this layer's id.
 * Project layers (`<ProjectTimeline />`, `<ProjectTrack />`) are evaluated at
 * the current frame only, so they throw inside. A `<FreezeFrame>` inside
 * another at the same frame with the same kind of children (a film that
 * freezes a frame where it plays this very `<FreezeFrame>`) would draw itself
 * forever, so it throws, naming the frames it is nested in; so does nesting
 * more than 16 deep.
 */
export function FreezeFrame(props: FreezeFrameProps): ReturnType<typeof React.createElement> {
  const context = React.useContext(CompositionRuntimeContext);
  const root = React.useContext(RootRuntimeContext);
  const ancestors = React.useContext(FreezeFrameChainContext);
  const [scope] = React.useState(() => ({}));
  if (!context || !root) {
    throw new Error('<FreezeFrame> must be called from within a Celesta <Composition>');
  }
  if (typeof props.frame !== 'number' || !Number.isFinite(props.frame)) {
    throw new Error('<FreezeFrame> requires a finite `frame`');
  }
  const time = secondsToTime(props.frame / root.fps);
  const children = React.Children.toArray(props.children).map((child) =>
    React.isValidElement(child) ? child.type : typeof child,
  );
  const chain = [...ancestors, { frame: props.frame, time: time.value, id: props.id, children }];
  // React's reconciler grows the tree without recursing, so a cycle would
  // run until the heap is gone; name it here instead. Every frame is a
  // function of its time, so the same children at the same time inside
  // themselves repeat forever; other children there (a poster frozen at
  // frame 0 holding a still of its own) end.
  if (ancestors.some((ancestor) => ancestor.time === time.value && sameChildren(ancestor.children, children))) {
    throw new Error(
      `<FreezeFrame frame={${props.frame}}> draws itself: it is inside a <FreezeFrame> at the same frame ` +
        `(${describeFreezeChain(chain)})`,
    );
  }
  if (chain.length > MAX_FREEZE_FRAME_DEPTH) {
    throw new Error(
      `<FreezeFrame frame={${props.frame}}> is nested more than ${MAX_FREEZE_FRAME_DEPTH} deep ` +
        `(${describeFreezeChain(chain)})`,
    );
  }
  // A frame past the composition's end is drawn as if the video went on, as
  // render.ts's walker decides in `childFreezeContext`. The frozen clock is
  // the root's inside, so a `<Sequence>` there places its window against it.
  const frozen = {
    ...root,
    lang: context.lang,
    fontFallback: context.fontFallback,
    time,
    openEnd: secondsFromTime(time) >= root.durationInFrames / root.fps,
  };
  return React.createElement(
    RootRuntimeContext.Provider,
    { value: frozen },
    React.createElement(
      CompositionRuntimeContext.Provider,
      { value: frozen },
      React.createElement(
        FreezeFrameContext.Provider,
        { value: scope },
        React.createElement(
          FreezeFrameChainContext.Provider,
          { value: chain },
          React.createElement('freeze-frame', props),
        ),
      ),
    ),
  );
}

export interface LoopProps extends CommonProps {
  /** Default text language within this loop; inherited when omitted. */
  lang?: string;
  /** Where text's missing characters are drawn from; see `SequenceProps.fontFallback`. */
  fontFallback?: FontFallbackMode;
  /** Length of one cycle in frames: positive and finite, fractions allowed. */
  durationInFrames: number;
  /**
   * Frame the first cycle starts at, in the enclosing timeline's own frame
   * numbering, like `<Sequence from>`. Defaults to 0.
   */
  from?: number;
  /**
   * How many cycles play; a fraction cuts the last one short. Defaults to
   * repeating until the end of the enclosing window.
   */
  times?: number;
  children?: ReactNode;
}

/**
 * Plays its children over and over: every `durationInFrames` frames from
 * `from`, the children's clock starts again at 0, as if each cycle were a
 * `<Sequence>` of its own. `useCurrentFrame()`/`useCurrentTime()`,
 * `<Video>`, `<Audio>`, and `<Sequence>`s inside all restart together, and
 * `useVideoConfig().durationInFrames` is one cycle's length. Each cycle's
 * audio is a clip of its own, cut short where the loop ends. Only the
 * current cycle is mounted, so the picture depends on the frame alone.
 */
export function Loop(props: LoopProps): ReturnType<typeof React.createElement> {
  const { durationInFrames: period, times, children, ...sequence } = props;
  if (typeof period !== 'number' || !Number.isFinite(period) || period <= 0) {
    throw new Error('<Loop> requires a positive finite `durationInFrames`');
  }
  if (times !== undefined && (typeof times !== 'number' || Number.isNaN(times) || times <= 0)) {
    throw new Error('<Loop> `times` must be a positive number');
  }
  // The outer window bounds the loop; the inner one is the current cycle.
  return React.createElement(
    Sequence,
    times !== undefined && Number.isFinite(times) ? { ...sequence, durationInFrames: times * period } : sequence,
    React.createElement(LoopCycle, { period }, children),
  );
}

function LoopCycle(props: { period: number; children?: ReactNode }): ReturnType<typeof React.createElement> {
  const { period } = props;
  const { time, fps } = React.useContext(CompositionRuntimeContext)!;
  const localSeconds = secondsFromTime(time);
  // The cycle whose window contains this time, compared exactly as
  // `<Sequence>` and render.ts's walker compare its `from` and end.
  let cycle = Math.max(0, Math.floor((localSeconds * fps) / period));
  while (cycle > 0 && isBefore(localSeconds, (cycle * period) / fps)) cycle--;
  while (!isBefore(localSeconds, (cycle * period + period) / fps)) cycle++;
  return React.createElement(Sequence, { from: cycle * period, durationInFrames: period }, props.children);
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
  /**
   * Transposition in semitones, -24 to 24. Fractions are cents (0.5 is 50
   * cents). Defaults to 0. Either a plain number or a generated
   * `KeyframeAnimation` over seconds-into-the-sequence times, like
   * `playbackRate`.
   */
  pitch?: AnimatedNumber;
  /**
   * When true, `playbackRate` changes only how long the clip takes, and the
   * pitch is set by `pitch` alone. Defaults to false: speed and pitch move
   * together, as before.
   */
  preservePitch?: boolean;
  /**
   * Seconds the clip takes to rise from silence where it starts being
   * heard. Defaults to 0. The fade multiplies `volume`.
   */
  fadeIn?: number;
  /**
   * Seconds the clip takes to fall to silence where it stops being heard:
   * the end of its innermost `<Sequence>`, or of an outer one that cuts it
   * shorter. Defaults to 0. The fade multiplies `volume`.
   */
  fadeOut?: number;
  /**
   * The group this clip's sound belongs to, for other clips' `duck.under`.
   * Dialogue voices belong to `"voice"` unless told otherwise.
   */
  duckGroup?: string;
  /** Turns this clip down while clips of other groups play. */
  duck?: AudioDuck;
}

/**
 * How a clip ducks: while a clip of a group in `under` plays, the clip's
 * gain dips to `to`, easing down over `attack` seconds so it gets there as
 * the other clip starts, and back up over `release` seconds after it ends.
 * The dip multiplies `volume` and the fades.
 */
export interface AudioDuck {
  /** The group, or groups, to duck under. With several, their clips all count. */
  under: string | string[];
  /** The gain under them: a multiplier from 0 to 1, or `{ db }` at or below 0. Defaults to 0.3. */
  to?: number | { db: number };
  /**
   * Seconds the gain takes to come down, ending as the other clip starts.
   * Defaults to 0.15. 0 switches it instantly, which can click on some
   * material.
   */
  attack?: number;
  /**
   * Seconds the gain takes to come back up after the other clip ends.
   * Defaults to 0.4. 0 switches it instantly, which can click on some
   * material.
   */
  release?: number;
  /** Silences shorter than this many seconds stay ducked. Defaults to 0.6. */
  holdGap?: number;
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

// Layer primitives, so a source chain can tell `<Rect>` from a component the
// author named `Rect`.
for (const primitive of [Group, Image, Text, Rect, Video, Mask]) markPrimitive(primitive);
for (const wrapper of [Composition, Sequence, FreezeFrame, Loop]) markTransparent(wrapper);
markWindow(Sequence, 'Sequence');
markWindow(FreezeFrame, 'FreezeFrame');
markWindow(Loop, 'Loop');
markWindow(LoopCycle, 'LoopCycle');
