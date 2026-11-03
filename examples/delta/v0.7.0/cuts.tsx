import { Easings } from '@celesta.studio/react';
import { chromaticAberration, noise, pixelate } from '@celesta.studio/shader';
import { Path } from '@celesta.studio/shapes';
import { blur, clockWipe, crossfade, cube, defineTransition, dissolve, flip, iris, slide, wipe, zoom } from '@celesta.studio/transitions';
import type { TransitionDefinition } from '@celesta.studio/transitions';
import { trianglePoints } from '../components/Delta';

// No two cuts in the film are alike: every built-in type (iris and zoom
// both ways), and two defined here with defineTransition().
// All of them take CUT frames, so each chapter's downbeat stays where
// ChapterShell expects it.

/** The next scene opens out of a Δ that grows past the corners. */
export const deltaOpen = defineTransition({
  name: 'delta',
  present: ({ progress, width, height }) => {
    const size = 60 + progress ** 2 * Math.hypot(width, height) * 2.6;
    return {
      enter: { mask: <Path x={width / 2} y={height / 2 + size * 0.12} points={trianglePoints(size)} closed fill="#FFFFFF" /> },
      exit: { scale: 1 + 0.25 * progress, x: -width * 0.125 * progress, y: -height * 0.125 * progress },
    };
  },
});

/** Both scenes tear: the channels split, cells grow, grain comes up. */
export const glitchCut = defineTransition({
  name: 'glitch',
  present: ({ progress }) => {
    const peak = Math.sin(Math.PI * progress);
    return {
      enter: {
        opacity: progress < 0.5 ? 0 : 1,
        shader: [chromaticAberration({ offset: 40 * peak, angle: 0 }), pixelate(1 + 30 * peak), noise({ amount: 0.5 * peak, seed: 7 })],
      },
      exit: { shader: [chromaticAberration({ offset: 40 * peak, angle: 180 }), pixelate(1 + 30 * peak)] },
    };
  },
});

export interface Cut {
  type: TransitionDefinition | 'wipe' | 'slide';
  from?: 'left' | 'right' | 'top' | 'bottom';
  /** Shown on the TRANSITIONS chapter's list. */
  label: string;
}

/** Cut `i` lands on chapter `i` (the last one on the outro). */
export const CUTS: readonly Cut[] = [
  { type: deltaOpen, label: 'defineTransition: delta' },
  { type: iris({ feather: 30 }), label: 'iris' },
  { type: clockWipe(), label: 'clock-wipe' },
  { type: zoom({ scale: 1.3 }), label: 'zoom' },
  { type: dissolve({ cellSize: 40, seed: 3 }), label: 'dissolve' },
  { type: blur({ amount: 40 }), label: 'blur' },
  { type: flip(), from: 'right', label: 'flip' },
  { type: cube(), from: 'right', label: 'cube' },
  { type: glitchCut, label: 'defineTransition: glitch' },
  { type: slide(), from: 'bottom', label: 'slide' },
  { type: wipe(), from: 'left', label: 'wipe' },
  { type: iris({ direction: 'close', feather: 30 }), label: 'iris close' },
  { type: zoom({ direction: 'out', scale: 1.4 }), label: 'zoom out' },
  { type: crossfade(), label: 'crossfade' },
];

export const CUT_EASING = Easings.easeInOutCubic;
