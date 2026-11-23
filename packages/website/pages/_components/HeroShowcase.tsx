'use client';

import { Pause, Play } from 'lucide-react';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';
import {
  clipAtFrame,
  DEFAULT_DARK_PALETTE,
  drawHeroStage,
  formatHeroTimecode,
  frameToRatio,
  HERO_CLIPS,
  HERO_FPS,
  HERO_REDUCED_MOTION_FRAME,
  HERO_STAGE_HEIGHT,
  HERO_STAGE_WIDTH,
  HERO_TOTAL_FRAMES,
  normalizeFrame,
  ratioToFrame,
  type HeroPalette,
} from './hero-stage-draw.ts';

export interface HeroShowcaseLabels {
  clips: readonly [string, string, string];
  stageLabel: string;
  scrubberLabel: string;
  play: string;
  pause: string;
  tcPrefix: string;
}

function readPalette(): HeroPalette {
  const style = getComputedStyle(document.documentElement);
  const get = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    bg: get('--nr-bg', DEFAULT_DARK_PALETTE.bg),
    panel: get('--nr-panel', DEFAULT_DARK_PALETTE.panel),
    raised: get('--nr-raised', DEFAULT_DARK_PALETTE.raised),
    line: get('--nr-line', DEFAULT_DARK_PALETTE.line),
    lineStrong: get('--nr-line-strong', DEFAULT_DARK_PALETTE.lineStrong),
    fg: get('--nr-fg', DEFAULT_DARK_PALETTE.fg),
    muted: get('--nr-muted', DEFAULT_DARK_PALETTE.muted),
    soft: get('--nr-soft', DEFAULT_DARK_PALETTE.soft),
    accent: get('--nr-accent', DEFAULT_DARK_PALETTE.accent),
    accentText: get('--nr-accent-text', DEFAULT_DARK_PALETTE.accentText),
    onAccent: get('--nr-on-accent', DEFAULT_DARK_PALETTE.onAccent),
    sansFont: get('--nr-font-sans', DEFAULT_DARK_PALETTE.sansFont),
    monoFont: get('--nr-font-mono', DEFAULT_DARK_PALETTE.monoFont),
  };
}

/**
 * Paired hero live stage (top-right on desktop, above the timeline on mobile) and interactive
 * 10-second timeline rail, driven by one shared 30 fps clock.
 *
 * Returns two direct grid children (`lp-stage` and `lp-timeline`) so `.lp-hero-grid` can position
 * them independently across desktop and mobile without duplicating state.
 */
export function HeroShowcase({ labels }: { labels: HeroShowcaseLabels }) {
  const [frame, setFrame] = useState(HERO_REDUCED_MOTION_FRAME);
  const [playing, setPlaying] = useState(false);
  const [hoverRatio, setHoverRatio] = useState<number | null>(null);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const railRef = useRef<HTMLDivElement>(null);
  const paletteRef = useRef<HeroPalette>(DEFAULT_DARK_PALETTE);
  const frameRef = useRef(HERO_REDUCED_MOTION_FRAME);
  const scrubbingRef = useRef(false);
  const resumeAfterScrubRef = useRef(false);

  const paint = useCallback((targetFrame: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const targetW = Math.round(HERO_STAGE_WIDTH * dpr);
    const targetH = Math.round(HERO_STAGE_HEIGHT * dpr);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawHeroStage(ctx, targetFrame, paletteRef.current);
  }, []);

  // Sync palette with Noir light/dark theme changes and start playback unless reduced motion is set.
  useEffect(() => {
    const refreshPalette = () => {
      paletteRef.current = readPalette();
      paint(frameRef.current);
    };
    refreshPalette();

    const observer = new MutationObserver(refreshPalette);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] });
    const schemeMedia = matchMedia('(prefers-color-scheme: dark)');
    schemeMedia.addEventListener('change', refreshPalette);

    const motionMedia = matchMedia('(prefers-reduced-motion: reduce)');
    if (!motionMedia.matches) {
      frameRef.current = 0;
      setFrame(0);
      setPlaying(true);
    }

    return () => {
      observer.disconnect();
      schemeMedia.removeEventListener('change', refreshPalette);
    };
  }, [paint]);

  // Paint whenever `frame` changes.
  useEffect(() => {
    frameRef.current = frame;
    paint(frame);
  }, [frame, paint]);

  // Single 30 fps playback clock, paused while the tab is hidden or the user is scrubbing.
  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    let startTime = performance.now();
    let baseFrame = frameRef.current;

    const tick = (now: number) => {
      if (document.hidden) {
        startTime = now;
        baseFrame = frameRef.current;
      } else {
        const elapsedFrames = Math.floor(((now - startTime) * HERO_FPS) / 1000);
        const next = normalizeFrame(baseFrame + elapsedFrames);
        if (next !== frameRef.current) {
          frameRef.current = next;
          setFrame(next);
        }
      }
      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing]);

  const ratioFromClientX = (clientX: number): number => {
    const rail = railRef.current;
    if (!rail) return 0;
    const rect = rail.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  };

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 && event.pointerType === 'mouse') return;
    // Keep keyboard `:focus-visible` clean without showing a focus ring on pointer scrub.
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    scrubbingRef.current = true;
    resumeAfterScrubRef.current = playing;
    setPlaying(false);
    const next = ratioToFrame(ratioFromClientX(event.clientX));
    frameRef.current = next;
    setFrame(next);
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const ratio = ratioFromClientX(event.clientX);
    setHoverRatio(ratio);
    if (!scrubbingRef.current) return;
    const next = ratioToFrame(ratio);
    if (next !== frameRef.current) {
      frameRef.current = next;
      setFrame(next);
    }
  };

  const finishScrub = () => {
    if (!scrubbingRef.current) return;
    scrubbingRef.current = false;
    if (resumeAfterScrubRef.current) setPlaying(true);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 10 : 1;
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      setPlaying(false);
      setFrame((current) => normalizeFrame(current + step));
    } else if (event.key === 'ArrowLeft') {
      event.preventDefault();
      setPlaying(false);
      setFrame((current) => normalizeFrame(current - step));
    } else if (event.key === 'Home') {
      event.preventDefault();
      setPlaying(false);
      setFrame(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      setPlaying(false);
      setFrame(HERO_TOTAL_FRAMES - 1);
    } else if (event.key === ' ' || event.key === 'Enter') {
      event.preventDefault();
      setPlaying((current) => !current);
    }
  };

  const activeClip = clipAtFrame(frame);
  const activeClipLabel = labels.clips[activeClip.index];
  const progressRatio = frameToRatio(frame);
  const timecode = formatHeroTimecode(frame);

  return <>
    <div className="lp-stage" role="region" aria-label={labels.stageLabel}>
      <div className="lp-stage-bar nr-mono" aria-hidden="true">
        <span className="lp-stage-clip">
          <i className="lp-stage-lamp" data-playing={playing ? '' : undefined} />
          {activeClipLabel}
        </span>
        <span className="lp-stage-meta"><span className="lp-stage-fps">{HERO_FPS} FPS · </span>{timecode}</span>
      </div>
      <div className="lp-stage-viewport">
        <canvas
          ref={canvasRef}
          className="lp-stage-canvas"
          width={HERO_STAGE_WIDTH}
          height={HERO_STAGE_HEIGHT}
          aria-hidden="true"
        />
      </div>
    </div>

    <div className="lp-timeline">
      <div
        ref={railRef}
        className="nr-rail lp-rail"
        role="slider"
        tabIndex={0}
        aria-label={labels.scrubberLabel}
        aria-valuemin={0}
        aria-valuemax={HERO_TOTAL_FRAMES - 1}
        aria-valuenow={frame}
        aria-valuetext={`${timecode} — ${activeClipLabel}`}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={finishScrub}
        onPointerCancel={finishScrub}
        onPointerLeave={() => { if (!scrubbingRef.current) setHoverRatio(null); }}
        onKeyDown={handleKeyDown}
      >
        {HERO_CLIPS.map((clip) => {
          const left = (clip.startFrame / HERO_TOTAL_FRAMES) * 100;
          const width = ((clip.endFrame - clip.startFrame) / HERO_TOTAL_FRAMES) * 100;
          const active = clip.index === activeClip.index;
          return <span
            key={clip.id}
            className="lp-rail-clip"
            data-active={active ? '' : undefined}
            style={{ left: `calc(${left}% + 4px)`, width: `calc(${width}% - 8px)` }}
          >
            {labels.clips[clip.index]}
          </span>;
        })}
        {hoverRatio !== null && (
          <span
            className="lp-rail-hover"
            aria-hidden="true"
            style={{ left: `${hoverRatio * 100}%` }}
          />
        )}
        <span
          className="nr-rail-head lp-rail-head"
          aria-hidden="true"
          style={{ left: `calc(${progressRatio * 100}% - 1px)` }}
        />
      </div>
      <div className="nr-rail-tc lp-rail-tc">
        <span className="lp-rail-left">
          <button
            type="button"
            className="lp-rail-toggle"
            aria-label={playing ? labels.pause : labels.play}
            aria-pressed={playing}
            onClick={() => setPlaying((current) => !current)}
          >
            {playing
              ? <Pause size={11} fill="currentColor" aria-hidden="true" />
              : <Play size={11} fill="currentColor" aria-hidden="true" />}
          </button>
          <span>00:00</span>
        </span>
        <b>{labels.tcPrefix} {timecode}</b>
        <span>00:10</span>
      </div>
    </div>
  </>;
}
