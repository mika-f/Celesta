import type { Scene, Layer, Asset, Paint, TextStyle } from './types';
import { loadFonts, releaseFonts } from './fonts';
import { textLineRanges, textStyle } from './text-layout';

type Canvas = HTMLCanvasElement;
type Context = CanvasRenderingContext2D;
type WebLayer = Layer;

const blend: Record<string, GlobalCompositeOperation> = {
  normal: 'source-over', multiply: 'multiply', screen: 'screen',
  overlay: 'overlay', add: 'lighter', difference: 'difference',
};

function paint(ctx: Context, value: Paint, x = 0, y = 0): string | CanvasGradient {
  if (value.type === 'solid') return value.color;
  const gradient = value.type === 'linear'
    ? ctx.createLinearGradient(x + value.start.x, y + value.start.y, x + value.end.x, y + value.end.y)
    : ctx.createRadialGradient(x + value.center.x, y + value.center.y, 0, x + value.center.x, y + value.center.y, value.radius);
  for (const stop of value.stops) gradient.addColorStop(stop.offset, stop.color);
  return gradient;
}

export class SceneCanvas {
  private images = new Map<string, Promise<HTMLImageElement>>();
  private videos = new Map<string, Promise<HTMLVideoElement>>();
  private urls = new Map<File, string>();
  private styledText = new Map<string, HTMLCanvasElement>();
  private textLayouts = new Map<string, { mask: HTMLCanvasElement; stroke?: HTMLCanvasElement; probe?: ImageData; regions: { start: number; end: number; x: number; width: number }[] }>();
  private fontKey = '';
  private usedLayouts = new Set<string>();
  private usedText = new Set<string>();

  constructor(private assets: Map<string, File> = new Map(), private baseURL?: string) {}

  dispose() {
    this.styledText.clear();
    this.textLayouts.clear();
    this.usedLayouts.clear();
    this.usedText.clear();
    releaseFonts(this.urls.values());
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear();
    for (const video of this.videos.values()) void video.then(v => { v.pause(); v.removeAttribute('src'); v.load(); });
  }

  private src(asset: Asset): string {
    const location = asset.location;
    if (location.type === 'url') return location.url;
    const file = this.assets.get(location.path) ?? this.assets.get(location.path.replace(/^\.\//, ''));
    if (!file) {
      if (this.baseURL) return new URL(location.path, this.baseURL).href;
      throw new Error(`Missing media file: ${location.path}. Add it with “Add media”.`);
    }
    let url = this.urls.get(file);
    if (!url) { url = URL.createObjectURL(file); this.urls.set(file, url); }
    return url;
  }

  async audioBytes(src: string): Promise<ArrayBuffer> {
    const file = this.assets.get(src) ?? this.assets.get(src.replace(/^\.\//, ''));
    if (file) return file.arrayBuffer();
    if (this.baseURL) src = new URL(src, this.baseURL).href;
    if (!/^https?:\/\//i.test(src)) throw new Error(`Missing audio file: ${src}. Add it with “Add media”.`);
    const response = await fetch(src);
    if (!response.ok) throw new Error(`Could not load audio: ${src} (${response.status}).`);
    return response.arrayBuffer();
  }

  private async image(asset: Asset): Promise<HTMLImageElement> {
    const key = JSON.stringify(asset.location);
    let pending = this.images.get(key);
    if (!pending) {
      pending = new Promise((resolve, reject) => {
        const image = new Image();
        image.crossOrigin = 'anonymous';
        image.onload = () => resolve(image);
        image.onerror = () => reject(new Error(`Could not load image: ${asset.id}`));
        image.src = this.src(asset);
      });
      this.images.set(key, pending);
    }
    return pending;
  }

  /** Shape the complete line once, then apply paint and reveals to its glyph mask. */
  private styledLine(ctx: Context, text: string, style: TextStyle, width: number, height: number, baseline: number, x: number, offset: number, originX: number, originY: number): HTMLCanvasElement {
    const { colorRuns, visibleCharacters, fill, ...geometry } = style;
    const lang = ('lang' in ctx && typeof ctx.lang === 'string' ? ctx.lang : undefined) || style.lang?.trim() || navigator.language;
    const layoutKey = JSON.stringify({ text, geometry, width, height, baseline, x, originX, originY, lang });
    const key = JSON.stringify({ layoutKey, colorRuns, visibleCharacters, fill, offset });
    this.usedLayouts.add(layoutKey);
    this.usedText.add(key);
    const cached = this.styledText.get(key);
    if (cached) return cached;
    let layout = this.textLayouts.get(layoutKey);
    if (!layout) {
      const mask = document.createElement('canvas');
      mask.width = Math.max(1, Math.ceil(width));
      mask.height = Math.max(1, Math.ceil(height));
      const draw = mask.getContext('2d')!;
      draw.font = ctx.font;
      draw.letterSpacing = ctx.letterSpacing;
      draw.fontKerning = ctx.fontKerning;
      draw.textBaseline = 'alphabetic';
      draw.fillStyle = '#fff';
      if ('lang' in draw && 'lang' in ctx) draw.lang = ctx.lang;
      let stroke: HTMLCanvasElement | undefined;
      if (style.stroke?.width) {
        stroke = document.createElement('canvas');
        stroke.width = mask.width;
        stroke.height = mask.height;
        const outline = stroke.getContext('2d')!;
        outline.font = draw.font;
        outline.letterSpacing = draw.letterSpacing;
        outline.fontKerning = draw.fontKerning;
        outline.textBaseline = 'alphabetic';
        if ('lang' in outline) outline.lang = lang;
        outline.lineJoin = 'round';
        outline.lineWidth = style.stroke.width * 2;
        outline.strokeStyle = paint(outline, style.stroke.paint, originX, originY);
        outline.strokeText(text, x, baseline);
      }
      draw.fillText(text, x, baseline);
      // A black/white probe distinguishes actual color glyphs from monochrome
      // emoji fallbacks, including white pixels within color emoji.
      let probe: ImageData | undefined;
      if (/\p{Emoji_Presentation}|\uFE0F|\u20e3/u.test(text)) {
        const pixels = draw.getImageData(0, 0, mask.width, mask.height);
        draw.clearRect(0, 0, mask.width, mask.height);
        draw.fillStyle = '#000';
        draw.fillText(text, x, baseline);
        probe = draw.getImageData(0, 0, mask.width, mask.height);
        draw.putImageData(pixels, 0, 0);
      }
      // SVG exposes character extents from the full shaped line, unlike
      // Canvas's prefix-only measurement. Fill changes never touch this node.
      const ns = 'http://www.w3.org/2000/svg';
      const svg = document.createElementNS(ns, 'svg');
      svg.style.cssText = 'position:fixed;left:-100000px;opacity:0;pointer-events:none';
      const element = document.createElementNS(ns, 'text');
      element.style.font = ctx.font;
      element.style.letterSpacing = ctx.letterSpacing;
      element.style.fontKerning = ctx.fontKerning;
      element.style.whiteSpace = 'pre';
      element.setAttribute('x', String(x));
      element.setAttribute('y', String(baseline));
      element.setAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:lang', lang);
      element.textContent = text;
      svg.append(element);
      document.body.append(svg);
      const regions: { start: number; end: number; x: number; width: number }[] = [];
      try {
        let point = 0;
        for (const { segment, index } of new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text)) {
          const extent = element.getExtentOfChar(index);
          const count = Array.from(segment).length;
          const previous = regions.at(-1);
          // SVG reports the same extent for each character of a ligature.
          if (previous && previous.x === extent.x && previous.width === extent.width) previous.end += count;
          else regions.push({ start: point, end: point + count, x: extent.x, width: extent.width });
          point += count;
        }
      } finally { svg.remove(); }
      layout = { mask, stroke, probe, regions };
      this.textLayouts.set(layoutKey, layout);
    }
    const result = document.createElement('canvas');
    result.width = layout.mask.width;
    result.height = layout.mask.height;
    const draw = result.getContext('2d')!;
    draw.fillStyle = fill ? paint(draw, fill, originX, originY) : '#fff';
    draw.fillRect(0, 0, result.width, result.height);
    const hidden = new Uint8Array(result.width);
    const leftEdge = layout.regions.reduce((left, region) => Math.min(left, region.x), Infinity);
    const rightEdge = layout.regions.reduce((right, region) => Math.max(right, region.x + region.width), -Infinity);
    let runIndex = 0;
    for (const region of layout.regions) {
      const point = offset + region.start;
      const runs = colorRuns ?? [];
      while (runIndex < runs.length && runs[runIndex].end <= point) runIndex++;
      const run = runs[runIndex];
      // Resolve once per cluster; shaping and rasterization are already cached.
      const color = run && run.start <= point && point < run.end ? run.color : undefined;
      const left = region.x === leftEdge ? 0 : Math.max(0, Math.min(result.width, Math.round(region.x)));
      const right = region.x + region.width === rightEdge ? result.width : Math.max(0, Math.min(result.width, Math.round(region.x + region.width)));
      if (color) {
        draw.clearRect(left, 0, right - left, result.height);
        draw.fillStyle = color;
        draw.fillRect(left, 0, right - left, result.height);
      }
      if (visibleCharacters != null && point >= visibleCharacters) hidden.fill(1, left, right);
    }
    const colors = draw.getImageData(0, 0, result.width, result.height);
    const pixels = layout.mask.getContext('2d')!.getImageData(0, 0, result.width, result.height);
    for (let i = 0; i < pixels.data.length; i += 4) {
      if (hidden[(i / 4) % result.width]) pixels.data.fill(0, i, i + 4);
      else if (pixels.data[i] === 255 && pixels.data[i + 1] === 255 && pixels.data[i + 2] === 255
        && (!layout.probe || layout.probe.data[i] !== 255 || layout.probe.data[i + 1] !== 255 || layout.probe.data[i + 2] !== 255)) {
        pixels.data[i] = colors.data[i];
        pixels.data[i + 1] = colors.data[i + 1];
        pixels.data[i + 2] = colors.data[i + 2];
        pixels.data[i + 3] = Math.round(pixels.data[i + 3] * colors.data[i + 3] / 255);
      }
    }
    draw.putImageData(pixels, 0, 0);
    if (layout.stroke) {
      const outline = layout.stroke.getContext('2d')!.getImageData(0, 0, result.width, result.height);
      for (let i = 0; i < outline.data.length; i += 4) {
        if (hidden[(i / 4) % result.width]) outline.data.fill(0, i, i + 4);
      }
      const stroke = document.createElement('canvas');
      stroke.width = result.width; stroke.height = result.height;
      stroke.getContext('2d')!.putImageData(outline, 0, 0);
      draw.globalCompositeOperation = 'destination-over';
      draw.drawImage(stroke, 0, 0);
    }
    if (visibleCharacters === 0) draw.clearRect(0, 0, result.width, result.height);
    this.styledText.set(key, result);
    return result;
  }

  private async video(asset: Asset): Promise<HTMLVideoElement> {
    const key = JSON.stringify(asset.location);
    let pending = this.videos.get(key);
    if (!pending) {
      pending = new Promise((resolve, reject) => {
        const video = document.createElement('video');
        video.crossOrigin = 'anonymous';
        video.preload = 'auto';
        video.muted = true;
        video.onloadeddata = () => resolve(video);
        video.onerror = () => reject(new Error(`Could not load video: ${asset.id}`));
        video.src = this.src(asset);
      });
      this.videos.set(key, pending);
    }
    return pending;
  }

  async draw(canvas: Canvas, scene: Scene): Promise<void> {
    this.usedLayouts.clear();
    this.usedText.clear();
    await loadFonts(scene.fonts ?? [], asset => this.src(asset));
    const fontKey = JSON.stringify(scene.fonts ?? []);
    if (fontKey !== this.fontKey) {
      this.styledText.clear();
      this.textLayouts.clear();
      this.fontKey = fontKey;
    }
    if (canvas.width !== scene.width) canvas.width = scene.width;
    if (canvas.height !== scene.height) canvas.height = scene.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Canvas 2D is unavailable.');
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = '#14161c';
    ctx.fillRect(0, 0, scene.width, scene.height);
    for (const layer of scene.layers) await this.layer(ctx, layer as WebLayer, new DOMMatrix(), 1);
    // Retain the active scene, even when it exceeds the idle-cache limit.
    for (const [cache, used, limit] of [[this.textLayouts, this.usedLayouts, 32], [this.styledText, this.usedText, 64]] as const) {
      for (const key of cache.keys()) {
        if (cache.size <= Math.max(limit, used.size)) break;
        if (!used.has(key)) cache.delete(key);
      }
    }
  }

  private async layer(ctx: Context, layer: WebLayer, parent: DOMMatrix, opacity: number): Promise<void> {
    const t = layer.transform;
    const matrix = parent.translate(t.position.x, t.position.y).rotate(t.rotation).scale(t.scale.x, t.scale.y);
    const alpha = Math.max(0, Math.min(1, opacity * layer.opacity));
    if (!alpha) return;
    const content = layer.content;
    const mode = blend[layer.blendMode ?? 'normal'];
    if (!mode) throw new Error(`Unsupported blend mode: ${layer.blendMode}`);
    const effects = layer.effects;
    if (effects && (effects.blur || effects.shadow || effects.glow)) {
      const source = this.surface(ctx);
      await this.layer(source, { ...layer, opacity: 1, blendMode: 'normal', effects: undefined }, parent, 1);
      const result = this.surface(ctx);
      // Each effect reads the unmodified source, matching the native renderer.
      for (const effect of [effects.shadow, effects.glow ? { ...effects.glow, offsetX: 0, offsetY: 0 } : null]) {
        if (!effect) continue;
        const mask = this.surface(ctx);
        mask.drawImage(source.canvas, 0, 0);
        mask.globalCompositeOperation = 'source-in';
        mask.fillStyle = effect.color;
        mask.fillRect(0, 0, mask.canvas.width, mask.canvas.height);
        result.filter = effect.blur > 0 ? `blur(${effect.blur}px)` : 'none';
        result.drawImage(mask.canvas, effect.offsetX, effect.offsetY);
        mask.canvas.width = 0;
      }
      result.filter = effects.blur && effects.blur > 0 ? `blur(${effects.blur}px)` : 'none';
      result.drawImage(source.canvas, 0, 0);
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.globalAlpha = alpha;
      ctx.globalCompositeOperation = mode;
      ctx.filter = 'none';
      ctx.drawImage(result.canvas, 0, 0);
      ctx.restore();
      source.canvas.width = result.canvas.width = 0;
      return;
    }
    if (content.type === 'group') {
      let target = ctx;
      if (mode !== 'source-over') target = this.surface(ctx);
      target.save();
      const clip = content.clip;
      if (clip) {
        target.setTransform(matrix);
        target.beginPath();
        target.roundRect(clip.x, clip.y, clip.width, clip.height, clip.cornerRadius ?? 0);
        target.clip();
      }
      for (const child of content.layers) {
        await this.layer(target, child as WebLayer, matrix, target === ctx ? alpha : 1);
      }
      target.restore();
      if (target !== ctx) {
        ctx.save();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.globalAlpha = alpha;
        ctx.globalCompositeOperation = mode;
        ctx.filter = 'none';
        ctx.drawImage(target.canvas, 0, 0);
        ctx.restore();
      }
      return;
    }

    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.globalCompositeOperation = mode;
    ctx.filter = 'none';
    if (content.type === 'rect') {
      const width = content.width, height = content.height;
      const x = -t.anchor.x * width, y = -t.anchor.y * height;
      const radius = Math.max(0, Math.min(content.cornerRadius, width / 2, height / 2));
      ctx.setTransform(matrix);
      if (content.fill) {
        ctx.beginPath();
        ctx.roundRect(x, y, width, height, radius);
        ctx.fillStyle = paint(ctx, content.fill, x, y);
        ctx.fill();
      }
      if (content.stroke?.width) {
        const inset = content.stroke.width / 2;
        ctx.beginPath();
        ctx.roundRect(x + inset, y + inset, width - 2 * inset, height - 2 * inset, Math.max(0, radius - inset));
        ctx.strokeStyle = paint(ctx, content.stroke.paint, x, y);
        ctx.lineWidth = content.stroke.width;
        ctx.stroke();
      }
    } else if (content.type === 'path') {
      ctx.setTransform(matrix);
      ctx.beginPath();
      let started = false;
      for (const command of content.commands) {
        if (!started && command.type !== 'moveTo') ctx.moveTo(0, 0);
        if (command.type === 'moveTo') ctx.moveTo(command.x, command.y);
        else if (command.type === 'lineTo') ctx.lineTo(command.x, command.y);
        else if (command.type === 'quadTo') ctx.quadraticCurveTo(command.x1, command.y1, command.x, command.y);
        else if (command.type === 'cubicTo') ctx.bezierCurveTo(command.x1, command.y1, command.x2, command.y2, command.x, command.y);
        else ctx.closePath();
        started = true;
      }
      if (content.fill) { ctx.fillStyle = paint(ctx, content.fill); ctx.fill(); }
      if (content.stroke?.width) {
        ctx.strokeStyle = paint(ctx, content.stroke.paint);
        ctx.lineWidth = content.stroke.width;
        ctx.lineCap = content.lineCap ?? 'butt';
        ctx.lineJoin = content.lineJoin ?? 'miter';
        ctx.miterLimit = content.miterLimit ?? 4;
        ctx.stroke();
      }
    } else if (content.type === 'text') {
      const style = content.style;
      const count = Array.from(content.text).length;
      let previousEnd = 0;
      for (const run of style.colorRuns ?? []) {
        if (!Number.isSafeInteger(run.start) || !Number.isSafeInteger(run.end) || run.start < previousEnd || run.end < run.start || run.end > count) {
          throw new Error('Invalid text color run range');
        }
        previousEnd = run.end;
      }
      if (style.visibleCharacters != null && (!Number.isSafeInteger(style.visibleCharacters) || style.visibleCharacters < 0)) {
        throw new Error('Text visibleCharacters must be a non-negative integer');
      }
      // Canvas language selection is available in newer browsers.
      const size = style.fontSize ?? 32;
      const lineHeight = style.lineHeight ?? size * 1.2;
      textStyle(ctx, style);
      ctx.textBaseline = 'alphabetic';
      ctx.textAlign = 'left';
      const lines = textLineRanges(ctx, content.text, content.maxWidth, style.lang);
      const measured = lines.map(line => ctx.measureText(line.text));
      const width = content.maxWidth ?? Math.max(0, ...measured.map(m => m.width));
      const placed = measured.map((m, i) => ({
        x: style.align === 'center' ? (width - m.width) / 2 : style.align === 'right' ? width - m.width : 0,
        baseline: i * lineHeight + (lineHeight - m.fontBoundingBoxAscent - m.fontBoundingBoxDescent) / 2 + m.fontBoundingBoxAscent,
      }));
      const multiline = /[\r\n]/.test(content.text);
      const top = multiline ? 0 : Math.min(...placed.map((p, i) => p.baseline - measured[i].actualBoundingBoxAscent));
      const bottom = multiline ? lines.length * lineHeight : Math.max(...placed.map((p, i) => p.baseline + measured[i].actualBoundingBoxDescent));
      const height = bottom - top;
      const y = content.baselineAnchor ? -(placed[0].baseline - top) : -t.anchor.y * height;
      ctx.setTransform(matrix.translate(-t.anchor.x * width, y));
      for (let i = 0; i < lines.length; i++) {
        const baseline = placed[i].baseline - top;
        if (style.colorRuns?.length || style.visibleCharacters != null) {
          const pad = Math.ceil(style.stroke?.width ?? 0) + 1;
          const m = measured[i];
          const left = Math.floor(placed[i].x - m.actualBoundingBoxLeft) - pad;
          const top = Math.floor(baseline - m.actualBoundingBoxAscent) - pad;
          const right = Math.ceil(placed[i].x + m.actualBoundingBoxRight) + pad;
          const bottom = Math.ceil(baseline + m.actualBoundingBoxDescent) + pad;
          const image = this.styledLine(ctx, lines[i].text, style, right - left, bottom - top, baseline - top, placed[i].x - left, lines[i].start, -left, -top);
          ctx.drawImage(image, left, top);
          continue;
        }
        if (style.stroke?.width) {
          ctx.lineJoin = 'round';
          ctx.lineWidth = style.stroke.width * 2;
          ctx.strokeStyle = paint(ctx, style.stroke.paint);
          ctx.strokeText(lines[i].text, placed[i].x, baseline);
        }
        ctx.fillStyle = style.fill ? paint(ctx, style.fill) : '#fff';
        ctx.fillText(lines[i].text, placed[i].x, baseline);
      }
    } else if (content.type === 'image' || content.type === 'video') {
      let image: HTMLImageElement | HTMLVideoElement;
      if (content.type === 'image') image = await this.image(content.asset);
      else {
        const video = await this.video(content.asset);
        image = video;
        const requested = Math.max(0, content.timing.sourceTimeSeconds);
        const end = Number.isFinite(video.duration) ? Math.max(0, video.duration - 0.001) : requested;
        const time = Math.min(requested, end);
        if (Math.abs(video.currentTime - time) > 0.001) {
          await new Promise<void>((resolve, reject) => {
            video.addEventListener('seeked', () => resolve(), { once: true });
            video.addEventListener('error', () => reject(new Error(`Could not seek video: ${content.asset.id}`)), { once: true });
            video.currentTime = time;
          });
        }
      }
      const width = image instanceof HTMLVideoElement ? image.videoWidth : image.naturalWidth;
      const height = image instanceof HTMLVideoElement ? image.videoHeight : image.naturalHeight;
      const w = content.type === 'image' ? content.width ?? (content.height === undefined ? width : content.height * width / height) : width;
      const h = content.type === 'image' ? content.height ?? w * height / width : height;
      ctx.setTransform(matrix.translate(-t.anchor.x * w, -t.anchor.y * h));
      if (content.type === 'image' && content.fit) {
        const scale = content.fit === 'contain' ? Math.min(w / width, h / height) : Math.max(w / width, h / height);
        ctx.beginPath();
        ctx.rect(0, 0, w, h);
        ctx.clip();
        ctx.drawImage(image, (w - width * scale) / 2, (h - height * scale) / 2, width * scale, height * scale);
      } else ctx.drawImage(image, 0, 0, w, h);
    } else {
      throw new Error(`The web renderer does not support ${content.type} layers yet.`);
    }
    ctx.restore();
  }

  private surface(ctx: Context): Context {
    const canvas = document.createElement('canvas');
    canvas.width = ctx.canvas.width;
    canvas.height = ctx.canvas.height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Canvas 2D is unavailable.');
    return context;
  }
}
