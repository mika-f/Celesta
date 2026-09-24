// A browser stand-in for the part of `@celesta/react` the playground scene
// uses. Vite and TypeScript alias `@celesta/react` here, so
// `title-scene.tsx` is the exact file visitors download, drawn onto a canvas
// with the native renderer's rules:
// - transforms are `translate · rotate · scale`, composed parent → child;
// - `anchorX`/`anchorY` pick the pivot as a fraction of the layer's size;
// - opacity multiplies down the tree and applies to each layer separately;
// - a rect's stroke is drawn inside its edge;
// - single-line text is trimmed to its visible glyphs before anchoring
//   (horizontally only without `maxWidth`).
import { Fragment, isValidElement, type ReactElement, type ReactNode } from 'react';

export { Easings, interpolate } from '../../../react/src/animation';

type Paint = { type: 'solid'; color: string };

export interface TextStyle {
  fontFamily?: string | null;
  fontSize?: number | null;
  fontWeight?: number | null;
  fill?: Paint | null;
  stroke?: { paint: Paint; width: number } | null;
  align?: 'left' | 'center' | 'right' | null;
  lineHeight?: number | null;
}

export interface CommonProps {
  id?: string;
  x?: number;
  y?: number;
  scale?: number;
  scaleX?: number;
  scaleY?: number;
  rotation?: number;
  anchorX?: number;
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

export interface RectProps extends CommonProps {
  width: number;
  height: number;
  fill?: string;
  stroke?: string;
  strokeWidth?: number;
  cornerRadius?: number;
}

export interface TextProps extends CommonProps {
  children: ReactNode;
  style?: TextStyle;
  maxWidth?: number;
}

export interface VideoConfig {
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
}

// Components are markers: `drawScene` recognizes them by identity.
export function Composition(_: CompositionProps): ReactNode { return null; }
export function Group(_: GroupProps): ReactNode { return null; }
export function Rect(_: RectProps): ReactNode { return null; }
export function Text(_: TextProps): ReactNode { return null; }

let current: { frame: number; config: VideoConfig } | null = null;

function runtime(name: string) {
  if (!current) throw new Error(`${name} must be called while drawing a scene`);
  return current;
}

export function useCurrentFrame(): number {
  return runtime('useCurrentFrame').frame;
}

export function useVideoConfig(): VideoConfig {
  return runtime('useVideoConfig').config;
}

/** Reads a composition's settings from its root component. */
export function compositionConfig(root: () => ReactNode): VideoConfig {
  const element = root();
  if (!isValidElement(element) || element.type !== Composition) {
    throw new Error('The root component must return a <Composition>');
  }
  const { width, height, fps, durationInFrames } = element.props as CompositionProps;
  return { width, height, fps, durationInFrames };
}

type Affine = readonly [number, number, number, number, number, number];
const IDENTITY: Affine = [1, 0, 0, 1, 0, 0];

function multiply(p: Affine, c: Affine): Affine {
  return [
    p[0] * c[0] + p[2] * c[1],
    p[1] * c[0] + p[3] * c[1],
    p[0] * c[2] + p[2] * c[3],
    p[1] * c[2] + p[3] * c[3],
    p[0] * c[4] + p[2] * c[5] + p[4],
    p[1] * c[4] + p[3] * c[5] + p[5],
  ];
}

function local(props: CommonProps): Affine {
  const radians = ((props.rotation ?? 0) * Math.PI) / 180;
  const sin = Math.sin(radians);
  const cos = Math.cos(radians);
  const scale = props.scale ?? 1;
  const scaleX = props.scaleX ?? scale;
  const scaleY = props.scaleY ?? scale;
  return [cos * scaleX, sin * scaleX, -sin * scaleY, cos * scaleY, props.x ?? 0, props.y ?? 0];
}

/** Places a layer of the given size: its anchor point lands on `x`/`y`. */
function place(ctx: CanvasRenderingContext2D, parent: Affine, props: CommonProps, width: number, height: number) {
  const m = multiply(multiply(parent, local(props)), [1, 0, 0, 1, -(props.anchorX ?? 0) * width, -(props.anchorY ?? 0) * height]);
  ctx.setTransform(m[0], m[1], m[2], m[3], m[4], m[5]);
}

function roundedRect(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number) {
  ctx.roundRect(x, y, Math.max(width, 0), Math.max(height, 0), Math.max(radius, 0));
}

function drawRect(ctx: CanvasRenderingContext2D, props: RectProps, parent: Affine, opacity: number) {
  const { width, height } = props;
  const radius = Math.min(Math.max(props.cornerRadius ?? 0, 0), width / 2, height / 2);
  const strokeWidth = props.stroke ? Math.max(props.strokeWidth ?? 0, 0) : 0;
  place(ctx, parent, props, width, height);
  ctx.globalAlpha = opacity;
  if (props.fill) {
    ctx.beginPath();
    roundedRect(ctx, strokeWidth, strokeWidth, width - strokeWidth * 2, height - strokeWidth * 2, radius - strokeWidth);
    ctx.fillStyle = props.fill;
    ctx.fill();
  }
  if (props.stroke && strokeWidth > 0) {
    ctx.beginPath();
    roundedRect(ctx, 0, 0, width, height, radius);
    roundedRect(ctx, strokeWidth, strokeWidth, width - strokeWidth * 2, height - strokeWidth * 2, radius - strokeWidth);
    ctx.fillStyle = props.stroke;
    ctx.fill('evenodd');
  }
}

function textContent(children: ReactNode): string {
  if (typeof children === 'string') return children;
  if (typeof children === 'number') return String(children);
  if (Array.isArray(children)) return children.map(textContent).join('');
  throw new Error('<Text> children must be a string, a number, or an array of those');
}

function wrap(ctx: CanvasRenderingContext2D, paragraph: string, maxWidth: number): string[] {
  const words = paragraph.split(/(?<=\s)/);
  const lines: string[] = [];
  let line = '';
  for (const word of words) {
    if (line && ctx.measureText((line + word).trimEnd()).width > maxWidth) {
      lines.push(line.trimEnd());
      line = word;
    } else {
      line += word;
    }
  }
  lines.push(line.trimEnd());
  return lines;
}

function drawText(ctx: CanvasRenderingContext2D, props: TextProps, parent: Affine, opacity: number) {
  const text = textContent(props.children);
  const style = props.style ?? {};
  const size = style.fontSize ?? 32;
  const lineHeight = style.lineHeight ?? size * 1.2;
  const family = style.fontFamily ? `"${style.fontFamily}", system-ui, sans-serif` : 'system-ui, sans-serif';
  ctx.font = `${style.fontWeight ?? 400} ${size}px ${family}`;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';

  const paragraphs = text.split('\n');
  const lines = props.maxWidth === undefined ? paragraphs : paragraphs.flatMap(p => wrap(ctx, p, props.maxWidth!));
  const measured = lines.map(line => ctx.measureText(line));
  const boxWidth = props.maxWidth ?? Math.max(...measured.map(m => m.width));
  const placed = measured.map((m, i) => {
    const x = style.align === 'center' ? (boxWidth - m.width) / 2 : style.align === 'right' ? boxWidth - m.width : 0;
    const baseline = i * lineHeight + (lineHeight - m.fontBoundingBoxAscent - m.fontBoundingBoxDescent) / 2 + m.fontBoundingBoxAscent;
    return { text: lines[i], x, baseline, m };
  });

  let left = 0, right = boxWidth, top = 0, bottom = lines.length * lineHeight;
  if (!text.includes('\n')) {
    top = Math.min(...placed.map(l => l.baseline - l.m.actualBoundingBoxAscent));
    bottom = Math.max(...placed.map(l => l.baseline + l.m.actualBoundingBoxDescent));
    if (props.maxWidth === undefined) {
      left = Math.min(...placed.map(l => l.x - l.m.actualBoundingBoxLeft));
      right = Math.max(...placed.map(l => l.x + l.m.actualBoundingBoxRight));
    }
  }

  place(ctx, parent, props, right - left, bottom - top);
  ctx.globalAlpha = opacity;
  for (const line of placed) {
    if (style.stroke && style.stroke.width > 0) {
      ctx.lineJoin = 'round';
      ctx.lineWidth = style.stroke.width * 2;
      ctx.strokeStyle = style.stroke.paint.color;
      ctx.strokeText(line.text, line.x - left, line.baseline - top);
    }
    ctx.fillStyle = style.fill?.color ?? '#ffffff';
    ctx.fillText(line.text, line.x - left, line.baseline - top);
  }
}

function draw(ctx: CanvasRenderingContext2D, node: ReactNode, parent: Affine, opacity: number) {
  if (Array.isArray(node)) {
    for (const child of node) draw(ctx, child, parent, opacity);
    return;
  }
  if (!isValidElement(node)) return;
  const element = node as ReactElement<Record<string, unknown>>;
  const props = element.props;
  const layerOpacity = opacity * Math.min(Math.max((props as CommonProps).opacity ?? 1, 0), 1);
  if (element.type === Fragment || element.type === Composition) {
    draw(ctx, props.children as ReactNode, parent, opacity);
  } else if (element.type === Group) {
    draw(ctx, props.children as ReactNode, multiply(parent, local(props)), layerOpacity);
  } else if (element.type === Rect) {
    drawRect(ctx, props as unknown as RectProps, parent, layerOpacity);
  } else if (element.type === Text) {
    drawText(ctx, props as unknown as TextProps, parent, layerOpacity);
  } else if (typeof element.type === 'function') {
    // Scene components here are plain functions of props and the frame.
    draw(ctx, (element.type as (p: unknown) => ReactNode)(props), parent, opacity);
  }
}

/** Draws one frame of `scene` onto a canvas sized to the composition. */
export function drawScene(canvas: HTMLCanvasElement, scene: ReactNode, config: VideoConfig, frame: number) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  if (canvas.width !== config.width) canvas.width = config.width;
  if (canvas.height !== config.height) canvas.height = config.height;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.clearRect(0, 0, config.width, config.height);
  current = { frame, config };
  try {
    draw(ctx, scene, IDENTITY, 1);
  } finally {
    current = null;
  }
}
