import type { Scene, Layer, Asset, Paint } from './types';
import { loadFonts, releaseFonts } from './fonts';
import { textLines, textStyle } from './text-layout';

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

  constructor(private assets: Map<string, File> = new Map(), private baseURL?: string) {}

  dispose() {
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
    await loadFonts(scene.fonts ?? [], asset => this.src(asset));
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
  }

  private async layer(ctx: Context, layer: WebLayer, parent: DOMMatrix, opacity: number): Promise<void> {
    const t = layer.transform;
    const matrix = parent.translate(t.position.x, t.position.y).rotate(t.rotation).scale(t.scale.x, t.scale.y);
    const alpha = Math.max(0, Math.min(1, opacity * layer.opacity));
    if (!alpha) return;
    const content = layer.content;
    const mode = blend[layer.blendMode ?? 'normal'];
    if (!mode) throw new Error(`Unsupported blend mode: ${layer.blendMode}`);
    if (content.type === 'group') {
      let target = ctx;
      if (mode !== 'source-over' || layer.effects) {
        const isolated = document.createElement('canvas');
        isolated.width = ctx.canvas.width;
        isolated.height = ctx.canvas.height;
        const isolatedContext = isolated.getContext('2d');
        if (!isolatedContext) throw new Error('Canvas 2D is unavailable.');
        target = isolatedContext;
      }
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
        this.effects(ctx, layer);
        ctx.drawImage(target.canvas, 0, 0);
        ctx.restore();
      }
      return;
    }

    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.globalCompositeOperation = mode;
    this.effects(ctx, layer);
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
      // Canvas language selection is available in newer browsers.
      const size = style.fontSize ?? 32;
      const lineHeight = style.lineHeight ?? size * 1.2;
      textStyle(ctx, style);
      ctx.textBaseline = 'alphabetic';
      ctx.textAlign = 'left';
      const lines = textLines(ctx, content.text, content.maxWidth);
      const measured = lines.map(line => ctx.measureText(line));
      const width = content.maxWidth ?? Math.max(0, ...measured.map(m => m.width));
      const placed = measured.map((m, i) => ({
        x: style.align === 'center' ? (width - m.width) / 2 : style.align === 'right' ? width - m.width : 0,
        baseline: i * lineHeight + (lineHeight - m.fontBoundingBoxAscent - m.fontBoundingBoxDescent) / 2 + m.fontBoundingBoxAscent,
      }));
      const top = content.text.includes('\n') ? 0 : Math.min(...placed.map((p, i) => p.baseline - measured[i].actualBoundingBoxAscent));
      const bottom = content.text.includes('\n') ? lines.length * lineHeight : Math.max(...placed.map((p, i) => p.baseline + measured[i].actualBoundingBoxDescent));
      const height = bottom - top;
      const y = content.baselineAnchor ? -(placed[0].baseline - top) : -t.anchor.y * height;
      ctx.setTransform(matrix.translate(-t.anchor.x * width, y));
      for (let i = 0; i < lines.length; i++) {
        const baseline = placed[i].baseline - top;
        if (style.stroke?.width) {
          ctx.lineJoin = 'round';
          ctx.lineWidth = style.stroke.width * 2;
          ctx.strokeStyle = paint(ctx, style.stroke.paint);
          ctx.strokeText(lines[i], placed[i].x, baseline);
        }
        ctx.fillStyle = style.fill ? paint(ctx, style.fill) : '#fff';
        ctx.fillText(lines[i], placed[i].x, baseline);
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

  private effects(ctx: Context, layer: Layer) {
    const effects = layer.effects;
    const filters: string[] = [];
    if (effects?.blur) filters.push(`blur(${effects.blur}px)`);
    if (effects?.shadow) {
      const { offsetX, offsetY, blur, color } = effects.shadow;
      filters.push(`drop-shadow(${offsetX}px ${offsetY}px ${blur}px ${color})`);
    }
    if (effects?.glow) filters.push(`drop-shadow(0px 0px ${effects.glow.blur}px ${effects.glow.color})`);
    ctx.filter = filters.join(' ') || 'none';
  }
}
