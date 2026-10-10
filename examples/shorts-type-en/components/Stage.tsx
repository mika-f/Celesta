import type { ReactNode } from 'react';
import { Easings, Group, Rect, progress, spring, useBeat, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { Camera, SafeArea } from '@celesta/layout';
import { Transition } from '@celesta/transitions';
import { BPM, C, H, SAFE, W } from '../constants';
import { halftone } from '../halftone';
import { Tape } from './Tape';

export type StageProps = {
  children: ReactNode;
  bg: string;
  // The scene's text colour and accent; the tapes are drawn in them.
  fg: string;
  accent: string;
  // Halftone dot colour, with alpha (#RRGGBBAA).
  tone: string;
  // Copy for the two tapes: a line of the manifesto and the API it shows.
  tapes: [string, string];
  // Where the halftone starts, as a fraction of the height.
  fade?: number;
  zoom?: number;
  rotation?: number;
  x?: number;
  y?: number;
  shake?: number;
};

// One scene's frame. The background, the halftone and the tapes fill the
// whole 1080×1920 canvas, running under the platform UI at the bottom and
// right; only the copy is laid out inside the safe area, filmed by a camera
// centred on it so a push-in grows evenly towards every edge of the safe
// area instead of towards the UI. The copy punches in on the cut (from
// smaller, so it never leaves the safe area) and lifts out over the last
// frames.
export function Stage({ children, bg, fg, accent, tone, tapes, fade, zoom = 1, rotation, x, y, shake }: StageProps) {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const { progress: phase, pulse } = useBeat({ bpm: BPM });
  const punch = spring({ frame: f, fps, config: { damping: 12, stiffness: 220 } });
  const bounce = 14 * pulse;
  return (
    <>
      <Rect width={W} height={H} fill={bg} shader={halftone({ time: f / fps, phase, tone, fade })} />
      <Tape y={H - 290 - bounce} rotation={-7} text={tapes[0]} bg={accent} fg={accent === C.ink ? C.paper : C.ink}
        speed={7} frame={f} />
      <Tape y={H - 130 + bounce} rotation={4} text={tapes[1]} bg={fg} fg={bg} speed={-5} frame={f} size={40} mono />
      <SafeArea padding={SAFE}>
        <Camera zoom={zoom * (0.9 + 0.1 * punch)} rotation={rotation} x={x} y={y} shake={shake} seed={7}>
          <Transition type={['fade', 'slide']} direction="out" slideFrom="top" distance={36}
            durationInFrames={8} easing={Easings.easeInCubic}>
            <Group blur={10 * (1 - progress(f, 0, 5))}>{children}</Group>
          </Transition>
        </Camera>
      </SafeArea>
    </>
  );
}
