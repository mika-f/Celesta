/**
 * Deterministic 30 fps canvas choreography for the landing page hero stage.
 * Pure functions only — every frame is a function of `(frame, palette)` so scrubbing
 * backward or jumping to any clip produces the exact same pixels.
 *
 * Strictly follows Noir: flat fills, zero border radius, 1 px hairlines, one electric blue.
 */

export const HERO_FPS = 30;
export const HERO_TOTAL_FRAMES = 300; // 10 seconds
export const HERO_STAGE_WIDTH = 640;
export const HERO_STAGE_HEIGHT = 360;
export const HERO_REDUCED_MOTION_FRAME = 52;

export interface HeroClip {
  readonly id: 'story' | 'atmosphere' | 'voice';
  readonly index: number;
  readonly startFrame: number;
  readonly endFrame: number; // exclusive
}

export const HERO_CLIPS: readonly HeroClip[] = [
  { id: 'story', index: 0, startFrame: 0, endFrame: 105 },
  { id: 'atmosphere', index: 1, startFrame: 105, endFrame: 205 },
  { id: 'voice', index: 2, startFrame: 205, endFrame: HERO_TOTAL_FRAMES },
];

export interface HeroPalette {
  bg: string;
  panel: string;
  raised: string;
  line: string;
  lineStrong: string;
  fg: string;
  muted: string;
  soft: string;
  accent: string;
  accentText: string;
  onAccent: string;
  sansFont: string;
  monoFont: string;
}

export const DEFAULT_DARK_PALETTE: HeroPalette = {
  bg: '#000000',
  panel: '#0d0d0f',
  raised: '#161618',
  line: '#1c1c1e',
  lineStrong: '#2c2c2e',
  fg: '#f5f5f7',
  muted: '#86868b',
  soft: '#a1a1a6',
  accent: '#3d7bff',
  accentText: '#7aa7ff',
  onAccent: '#000000',
  sansFont: '"Helvetica Neue", ui-sans-serif, system-ui, -apple-system, sans-serif',
  monoFont: 'ui-monospace, "SF Mono", SFMono-Regular, Menlo, Consolas, monospace',
};

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function normalizeFrame(frame: number): number {
  const floored = Math.floor(frame);
  return ((floored % HERO_TOTAL_FRAMES) + HERO_TOTAL_FRAMES) % HERO_TOTAL_FRAMES;
}

export function easeOutExpo(t: number): number {
  const x = clamp(t, 0, 1);
  return x >= 1 ? 1 : 1 - 2 ** (-10 * x);
}

export function easeInOutCubic(t: number): number {
  const x = clamp(t, 0, 1);
  return x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2;
}

export function easeOutCubic(t: number): number {
  const x = clamp(t, 0, 1);
  return 1 - (1 - x) ** 3;
}

export function interpolate(
  frame: number,
  input: readonly [number, number],
  output: readonly [number, number],
  easing: (t: number) => number = (t) => t,
): number {
  const [inStart, inEnd] = input;
  const [outStart, outEnd] = output;
  if (inEnd === inStart) return outEnd;
  const raw = clamp((frame - inStart) / (inEnd - inStart), 0, 1);
  const eased = easing(raw);
  return outStart + (outEnd - outStart) * eased;
}

/** `hh:mm:ss:ff` at 30 fps, wrapped cleanly to the 10-second loop (`00:00:00:00`..`00:00:09:29`). */
export function formatHeroTimecode(frame: number): string {
  const f = normalizeFrame(frame);
  const seconds = Math.floor(f / HERO_FPS);
  const parts = [
    Math.floor(seconds / 3600),
    Math.floor(seconds / 60) % 60,
    seconds % 60,
    f % HERO_FPS,
  ];
  return parts.map((part) => String(part).padStart(2, '0')).join(':');
}

export function clipAtFrame(frame: number): HeroClip {
  const f = normalizeFrame(frame);
  for (const clip of HERO_CLIPS) {
    if (f >= clip.startFrame && f < clip.endFrame) return clip;
  }
  return HERO_CLIPS[HERO_CLIPS.length - 1];
}

/** Progress within [0, 1] across the timeline for positioning the playhead. */
export function frameToRatio(frame: number): number {
  return normalizeFrame(frame) / (HERO_TOTAL_FRAMES - 1);
}

export function ratioToFrame(ratio: number): number {
  return clamp(Math.round(clamp(ratio, 0, 1) * (HERO_TOTAL_FRAMES - 1)), 0, HERO_TOTAL_FRAMES - 1);
}

/** Deterministic audio bar energy in [0.06, 1] for `voice.wav`. */
function voiceBarAmplitude(barIndex: number, barCount: number, localFrame: number): number {
  const x = barIndex / (barCount - 1); // 0..1
  const centerWindow = Math.sin(Math.PI * x) ** 0.72;
  const t = localFrame / HERO_FPS;
  // Rhythmic speech-like cadence with two pulse beats inside the 3.17s clip
  const beat1 = Math.exp(-(((t - 0.65) / 0.52) ** 2));
  const beat2 = Math.exp(-(((t - 1.85) / 0.68) ** 2));
  const envelope = 0.28 + 0.62 * beat1 + 0.72 * beat2;
  const harmonic =
    0.5
    + 0.28 * Math.sin(barIndex * 0.62 - t * 6.4)
    + 0.16 * Math.cos(barIndex * 1.15 + t * 4.1)
    + 0.1 * Math.sin(barIndex * 2.05 - t * 9.2);
  return clamp(centerWindow * envelope * harmonic, 0.06, 1);
}

function drawSafeFrame(ctx: CanvasRenderingContext2D, p: HeroPalette) {
  const W = HERO_STAGE_WIDTH;
  const H = HERO_STAGE_HEIGHT;
  const pad = 22;
  const arm = 12;

  // Subtle registration crosses in the background
  ctx.save();
  ctx.strokeStyle = p.line;
  ctx.lineWidth = 1;
  const cols = [W * 0.25, W * 0.5, W * 0.75];
  const rows = [H * 0.28, H * 0.5, H * 0.72];
  for (const cx of cols) {
    for (const cy of rows) {
      const x = Math.round(cx) + 0.5;
      const y = Math.round(cy) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x - 3.5, y);
      ctx.lineTo(x + 3.5, y);
      ctx.moveTo(x, y - 3.5);
      ctx.lineTo(x, y + 3.5);
      ctx.stroke();
    }
  }

  // Viewfinder corner brackets
  ctx.strokeStyle = p.lineStrong;
  ctx.lineWidth = 1;
  const corners: readonly [number, number, number, number][] = [
    [pad, pad, 1, 1],
    [W - pad, pad, -1, 1],
    [pad, H - pad, 1, -1],
    [W - pad, H - pad, -1, -1],
  ];
  for (const [x0, y0, dx, dy] of corners) {
    const x = Math.round(x0) + 0.5;
    const y = Math.round(y0) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x + dx * arm, y);
    ctx.lineTo(x, y);
    ctx.lineTo(x, y + dy * arm);
    ctx.stroke();
  }
  ctx.restore();
}

function drawTelemetry(
  ctx: CanvasRenderingContext2D,
  p: HeroPalette,
  leftFormula: string,
  frame: number,
) {
  const W = HERO_STAGE_WIDTH;
  const H = HERO_STAGE_HEIGHT;
  const y = H - 31;

  ctx.save();
  ctx.font = `500 11px ${p.monoFont}`;
  ctx.textBaseline = 'middle';

  ctx.fillStyle = p.soft;
  ctx.textAlign = 'left';
  ctx.fillText(leftFormula, 38, y);

  ctx.fillStyle = p.accentText;
  ctx.textAlign = 'right';
  ctx.fillText(`F ${String(frame).padStart(3, '0')} / ${HERO_TOTAL_FRAMES}`, W - 38, y);
  ctx.restore();
}

/** Scene 1 (`story.tsx`): kinetic typography reveal + Celesta canvas selection box. */
function drawStoryScene(
  ctx: CanvasRenderingContext2D,
  local: number,
  frame: number,
  p: HeroPalette,
) {
  const W = HERO_STAGE_WIDTH;
  const H = HERO_STAGE_HEIGHT;

  // Enter / exit phases
  const ruleIn = interpolate(local, [0, 22], [0, 1], easeOutExpo);
  const exitT = interpolate(local, [88, 104], [0, 1], easeInOutCubic);

  // Accent index bar and chapter caption
  const labelAlpha = interpolate(local, [4, 18], [0, 1], easeOutCubic) * (1 - exitT);
  const accentBarW = interpolate(local, [2, 26], [0, 56], easeOutExpo) * (1 - exitT);

  ctx.save();
  ctx.globalAlpha = clamp(labelAlpha, 0, 1);
  ctx.fillStyle = p.accent;
  ctx.fillRect(68, 76, accentBarW, 2);

  ctx.font = `500 11px ${p.monoFont}`;
  ctx.fillStyle = p.accentText;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.fillText('01  STORY.TSX', 68, 62);
  ctx.restore();

  // Horizontal editorial rule across the middle
  const ruleWidth = (W - 136) * ruleIn * (1 - exitT * 0.6);
  ctx.save();
  ctx.fillStyle = p.lineStrong;
  ctx.fillRect(68, Math.round(H * 0.5) + 48, ruleWidth, 1);
  ctx.restore();

  // Masked kinetic headline lines
  const line1Slide = interpolate(local, [6, 30], [54, 0], easeOutExpo) - exitT * 52;
  const line2Slide = interpolate(local, [14, 38], [54, 0], easeOutExpo) - exitT * 52;
  const line1Alpha = interpolate(local, [6, 20], [0, 1], easeOutCubic) * (1 - exitT);
  const line2Alpha = interpolate(local, [14, 28], [0, 1], easeOutCubic) * (1 - exitT);

  // Subtle horizontal nudge during the selection-box beat (frames 46..76)
  const nudgeX = interpolate(local, [48, 66], [0, 22], easeInOutCubic)
    - interpolate(local, [70, 84], [0, 22], easeInOutCubic);

  ctx.save();
  ctx.font = `800 50px ${p.sansFont}`;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';

  // Line 1 clip
  ctx.save();
  ctx.beginPath();
  ctx.rect(64, 92, W - 128, 58);
  ctx.clip();
  ctx.globalAlpha = clamp(line1Alpha, 0, 1);
  ctx.fillStyle = p.fg;
  ctx.fillText('EVERY FRAME', 68, 142 + line1Slide);
  ctx.restore();

  // Line 2 clip (interactive target)
  const line2X = 68 + nudgeX;
  const line2Baseline = 198 + line2Slide;
  ctx.save();
  ctx.beginPath();
  ctx.rect(64, 150, W - 128, 60);
  ctx.clip();
  ctx.globalAlpha = clamp(line2Alpha, 0, 1);
  ctx.fillStyle = p.fg;
  ctx.fillText('IS CODE.', line2X, line2Baseline);
  ctx.restore();

  const line2Metrics = ctx.measureText('IS CODE.');
  const boxW = Math.ceil(line2Metrics.width) + 20;
  ctx.restore();

  // Celesta editor selection overlay around "IS CODE."
  const selectIn = interpolate(local, [34, 46], [0, 1], easeOutCubic);
  const selectOut = interpolate(local, [78, 88], [1, 0], easeOutCubic);
  const selectAlpha = selectIn * selectOut;

  if (selectAlpha > 0.01) {
    const bx = Math.round(line2X - 10) + 0.5;
    const by = Math.round(154 + line2Slide) + 0.5;
    const bw = Math.round(boxW);
    const bh = 52;

    ctx.save();
    ctx.globalAlpha = selectAlpha;

    // Selection frame
    ctx.strokeStyle = p.accent;
    ctx.lineWidth = 1;
    ctx.strokeRect(bx, by, bw, bh);

    // 4 corner handles
    const hs = 5;
    const handles: readonly [number, number][] = [
      [bx, by],
      [bx + bw, by],
      [bx, by + bh],
      [bx + bw, by + bh],
    ];
    for (const [hx, hy] of handles) {
      ctx.fillStyle = p.bg;
      ctx.fillRect(hx - hs / 2, hy - hs / 2, hs, hs);
      ctx.strokeRect(hx - hs / 2, hy - hs / 2, hs, hs);
    }

    // Center pivot crosshair
    const pcx = Math.round(bx + bw / 2) + 0.5;
    const pcy = Math.round(by + bh / 2) + 0.5;
    ctx.beginPath();
    ctx.moveTo(pcx - 5, pcy);
    ctx.lineTo(pcx + 5, pcy);
    ctx.moveTo(pcx, pcy - 5);
    ctx.lineTo(pcx, pcy + 5);
    ctx.stroke();

    // Coordinate pill tag (square corners, Noir accent fill)
    const tagText = `x=${Math.round(line2X)}  y=${Math.round(line2Baseline)}`;
    ctx.font = `600 10.5px ${p.monoFont}`;
    const tagW = Math.ceil(ctx.measureText(tagText).width) + 12;
    const tagX = bx + bw + 8;
    const tagY = by;
    ctx.fillStyle = p.accent;
    ctx.fillRect(tagX, tagY, tagW, 20);
    ctx.fillStyle = p.onAccent;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(tagText, tagX + 6, tagY + 10.5);
    ctx.restore();
  }

  // Secondary caption below the rule
  const subAlpha = interpolate(local, [24, 38], [0, 1], easeOutCubic) * (1 - exitT);
  if (subAlpha > 0.01) {
    ctx.save();
    ctx.globalAlpha = subAlpha;
    ctx.font = `500 12px ${p.monoFont}`;
    ctx.fillStyle = p.soft;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText('const x = interpolate(frame, [48, 66], [68, 90])', 68, 250);
    ctx.restore();
  }

  const displayY = Math.round(line2Slide);
  const formula = local < 42
    ? `y = interpolate(f, [14, 38], [54, 0]) -> ${displayY >= 0 ? displayY : 0}px`
    : `x = ${Math.round(line2X)}px  opacity = ${line2Alpha.toFixed(2)}`;
  drawTelemetry(ctx, p, formula, frame);
}

/** Scene 2 (`atmosphere`): procedural multi-layer rotating wireframe ribbon. */
function drawAtmosphereScene(
  ctx: CanvasRenderingContext2D,
  local: number,
  frame: number,
  p: HeroPalette,
) {
  const W = HERO_STAGE_WIDTH;
  const H = HERO_STAGE_HEIGHT;
  const cx = W * 0.5;
  const cy = H * 0.5;

  const enterT = interpolate(local, [0, 22], [0, 1], easeOutExpo);
  // Collapse vertically into the center baseline so Scene 3's waveform rises from it.
  const collapseT = interpolate(local, [82, 99], [0, 1], easeInOutCubic);
  const scaleYFactor = Math.max(0.008, 1 - collapseT);
  const fadeHeader = interpolate(local, [2, 16], [0, 1], easeOutCubic) * (1 - collapseT);

  // Chapter label
  ctx.save();
  ctx.globalAlpha = clamp(fadeHeader, 0, 1);
  ctx.font = `500 11px ${p.monoFont}`;
  ctx.fillStyle = p.accentText;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.fillText('02  ATMOSPHERE', 38, 44);
  ctx.restore();

  // Subtle central axes
  ctx.save();
  ctx.strokeStyle = p.line;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(56, Math.round(cy) + 0.5);
  ctx.lineTo(W - 56, Math.round(cy) + 0.5);
  if (collapseT < 0.95) {
    ctx.moveTo(Math.round(cx) + 0.5, 56);
    ctx.lineTo(Math.round(cx) + 0.5, H - 56);
  }
  ctx.stroke();
  ctx.restore();

  // 16 phased wireframe rectangles creating an optical twisted tunnel
  const layerCount = 16;
  const baseDeg = local * 1.45;
  const accentLayer = Math.floor((local / 4) % layerCount);

  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(1, scaleYFactor);

  for (let i = 0; i < layerCount; i++) {
    const depth = i / (layerCount - 1); // 0 = inner, 1 = outer
    const layerEnter = interpolate(local - i * 0.8, [0, 18], [0, 1], easeOutExpo);
    if (layerEnter <= 0.01) continue;

    const size = (44 + depth * 176) * (0.45 + 0.55 * layerEnter);
    const width = size * (1.28 - 0.14 * Math.sin(local * 0.05 + depth * 2));
    const height = size * 0.78;
    const angleDeg = (baseDeg - i * 5.2) * (1 - collapseT);
    const angleRad = (angleDeg * Math.PI) / 180;

    const isLead = i === layerCount - 1 || i === accentLayer;
    ctx.save();
    ctx.rotate(angleRad);
    ctx.strokeStyle = isLead ? p.accent : p.fg;
    ctx.globalAlpha = isLead
      ? (i === layerCount - 1 ? 0.95 : 0.75) * enterT
      : (0.1 + depth * 0.36) * enterT * (1 - collapseT * 0.5);
    ctx.lineWidth = i === layerCount - 1 ? 1.5 : 1;
    ctx.strokeRect(-width / 2, -height / 2, width, height);
    ctx.restore();
  }

  // Two orbiting square nodes with coordinate drop-lines
  if (collapseT < 0.85) {
    const orbitAlpha = enterT * (1 - collapseT);
    const theta1 = (local * 0.055) - 0.5;
    const theta2 = theta1 + Math.PI;
    const rx = 205;
    const ry = 104;

    const nodes: readonly [number, number, boolean][] = [
      [Math.cos(theta1) * rx, Math.sin(theta1) * ry, true],
      [Math.cos(theta2) * rx, Math.sin(theta2) * ry, false],
    ];

    ctx.save();
    ctx.globalAlpha = orbitAlpha;
    for (const [nx, ny, primary] of nodes) {
      // Hairline guide back to axis
      ctx.strokeStyle = p.lineStrong;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(nx, 0);
      ctx.lineTo(nx, ny);
      ctx.stroke();

      // Square marker
      const s = primary ? 8 : 6;
      if (primary) {
        ctx.fillStyle = p.accent;
        ctx.fillRect(nx - s / 2, ny - s / 2, s, s);
      } else {
        ctx.fillStyle = p.bg;
        ctx.strokeStyle = p.fg;
        ctx.fillRect(nx - s / 2, ny - s / 2, s, s);
        ctx.strokeRect(nx - s / 2, ny - s / 2, s, s);
      }
    }
    ctx.restore();
  }

  ctx.restore();

  // Horizontal bridge line brightening at the end of Scene 2
  if (collapseT > 0.25) {
    ctx.save();
    ctx.globalAlpha = clamp((collapseT - 0.25) / 0.75, 0, 1);
    ctx.fillStyle = p.accent;
    ctx.fillRect(68, Math.round(cy), W - 136, 1.5);
    ctx.restore();
  }

  const normalizedAngle = ((baseDeg % 360) + 360) % 360;
  drawTelemetry(ctx, p, `rotation = ${normalizedAngle.toFixed(1)}deg  layers = ${layerCount}`, frame);
}

/** Scene 3 (`voice.wav`): deterministic audio waveform bars + scan playhead + stereo meters. */
function drawVoiceScene(
  ctx: CanvasRenderingContext2D,
  local: number,
  frame: number,
  p: HeroPalette,
) {
  const W = HERO_STAGE_WIDTH;
  const H = HERO_STAGE_HEIGHT;
  const cy = H * 0.5 + 4;
  const clipDuration = 95;

  const enterT = interpolate(local, [0, 16], [0, 1], easeOutExpo);
  const exitT = interpolate(local, [79, 94], [0, 1], easeInOutCubic);
  const ampScale = enterT * (1 - exitT);
  const scanProgress = clamp(local / (clipDuration - 1), 0, 1);

  // Header row: label left, stereo peak meters right
  const uiAlpha = interpolate(local, [2, 14], [0, 1], easeOutCubic) * (1 - exitT);
  ctx.save();
  ctx.globalAlpha = clamp(uiAlpha, 0, 1);
  ctx.font = `500 11px ${p.monoFont}`;
  ctx.fillStyle = p.accentText;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.fillText('03  VOICE.WAV', 38, 44);
  ctx.restore();

  const barCount = 36;
  const trackWidth = W - 136;
  const gap = 5;
  const barW = Math.floor((trackWidth - gap * (barCount - 1)) / barCount);
  const actualWidth = barW * barCount + gap * (barCount - 1);
  const startX = Math.round((W - actualWidth) / 2);
  const maxHalfH = 88;
  const scanX = startX + scanProgress * actualWidth;

  // Baseline
  ctx.save();
  ctx.fillStyle = p.lineStrong;
  ctx.fillRect(startX, Math.round(cy), actualWidth, 1);
  ctx.restore();

  let energySum = 0;
  let peak = 0;

  for (let i = 0; i < barCount; i++) {
    const amp = voiceBarAmplitude(i, barCount, local) * ampScale;
    energySum += amp * amp;
    if (amp > peak) peak = amp;

    const halfH = Math.max(1.5, amp * maxHalfH);
    const bx = startX + i * (barW + gap);
    const barCenter = bx + barW / 2;
    const distFromScan = Math.abs(barCenter - scanX);
    const nearHead = distFromScan < 26;
    const passed = barCenter <= scanX;

    ctx.save();
    if (nearHead) {
      ctx.fillStyle = p.accent;
      ctx.globalAlpha = 1;
    } else if (passed) {
      ctx.fillStyle = p.fg;
      ctx.globalAlpha = 0.78;
    } else {
      ctx.fillStyle = p.lineStrong;
      ctx.globalAlpha = 0.9;
    }
    ctx.fillRect(bx, Math.round(cy - halfH), barW, Math.max(2, Math.round(halfH * 2)));
    ctx.restore();
  }

  // Vertical scan playhead over the waveform
  if (ampScale > 0.05) {
    ctx.save();
    ctx.globalAlpha = clamp(ampScale * 1.2, 0, 1);
    ctx.fillStyle = p.fg;
    ctx.fillRect(Math.round(scanX), cy - maxHalfH - 12, 1.5, (maxHalfH + 12) * 2);
    // Tiny top square cap in accent
    ctx.fillStyle = p.accent;
    ctx.fillRect(Math.round(scanX) - 2, cy - maxHalfH - 16, 5.5, 4);
    ctx.restore();
  }

  // Compact L / R segmented level meters in top-right
  const rms = Math.sqrt(energySum / barCount);
  const segments = 12;
  const levelL = clamp(rms * 1.35, 0, 1);
  const levelR = clamp(rms * 1.22 + 0.08 * Math.sin(local * 0.4), 0, 1);

  if (uiAlpha > 0.01) {
    ctx.save();
    ctx.globalAlpha = uiAlpha;
    const meterRight = W - 38;
    const segW = 6;
    const segGap = 2;
    const meterTotalW = segments * segW + (segments - 1) * segGap;
    const meterLeft = meterRight - meterTotalW;

    ctx.font = `500 9.5px ${p.monoFont}`;
    ctx.fillStyle = p.muted;
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';

    const channels: readonly [string, number, number][] = [
      ['L', levelL, 40],
      ['R', levelR, 50],
    ];
    for (const [ch, lvl, rowY] of channels) {
      ctx.fillStyle = p.muted;
      ctx.fillText(ch, meterLeft - 6, rowY + 2.5);
      const lit = Math.round(lvl * segments);
      for (let s = 0; s < segments; s++) {
        ctx.fillStyle = s < lit ? (s >= segments - 2 ? p.accent : p.fg) : p.line;
        ctx.fillRect(meterLeft + s * (segW + segGap), rowY, segW, 5);
      }
    }
    ctx.restore();
  }

  const db = rms > 0.001 ? Math.max(-48, 20 * Math.log10(rms)).toFixed(1) : '-inf';
  drawTelemetry(ctx, p, `audio.rms = ${rms.toFixed(2)}  ${db} dB  48 kHz`, frame);
}

/** Draws a single deterministic frame onto a `640 × 360` logical coordinate space. */
export function drawHeroStage(
  ctx: CanvasRenderingContext2D,
  frame: number,
  palette: HeroPalette,
): void {
  const f = normalizeFrame(frame);
  const clip = clipAtFrame(f);

  ctx.save();
  ctx.fillStyle = palette.bg;
  ctx.fillRect(0, 0, HERO_STAGE_WIDTH, HERO_STAGE_HEIGHT);

  drawSafeFrame(ctx, palette);

  if (clip.id === 'story') {
    drawStoryScene(ctx, f - clip.startFrame, f, palette);
  } else if (clip.id === 'atmosphere') {
    drawAtmosphereScene(ctx, f - clip.startFrame, f, palette);
  } else {
    drawVoiceScene(ctx, f - clip.startFrame, f, palette);
  }

  ctx.restore();
}
