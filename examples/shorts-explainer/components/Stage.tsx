import { Easings, Group, Rect, interpolate, useCurrentFrame } from '@celesta/react';
import { CharacterView } from '@celesta/character';
import { CAST } from '../character';
import { C, SAFE, STAGE_Y, TAG_Y } from '../constants';
import type { SpeakerId } from '../script';
import { lineAt, plan } from '../voice';
import { Label } from './Label';

// Where each portrait stands. The PSDs are both 2400 px tall but draw their
// heads at different sizes, so each gets its own scale and offset.
const SPOTS: Record<SpeakerId, { x: number; y: number; scale: number; tagX: number; tagAlign: 0 | 1 }> = {
  kiritan: { x: 300, y: STAGE_Y - 150, scale: 0.6, tagX: SAFE.left, tagAlign: 0 },
  zunko: { x: 790, y: STAGE_Y - 120, scale: 0.84, tagX: SAFE.right, tagAlign: 1 },
};

// How much `speaker` is "the one talking" at `frame`, 0–1, easing over the
// first frames of each line so the focus slides from one portrait to the other.
function focus(speaker: SpeakerId, frame: number) {
  const current = lineAt(frame);
  const previous = plan.lines[current.index - 1];
  const was = previous ? Number(previous.line.speaker === speaker) : 0;
  const is = Number(current.line.speaker === speaker);
  const t = interpolate(frame, [current.from - current.leadInFrames, current.from + 4], [0, 1], {
    easing: Easings.easeOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  return was + (is - was) * t;
}

// The two portraits in the bottom half. The speaker steps forward; the
// listener sinks back a little.
export function Stage() {
  const frame = useCurrentFrame();
  return (
    <>
      {(Object.keys(SPOTS) as SpeakerId[]).map((id) => {
        const spot = SPOTS[id];
        const f = focus(id, frame);
        // Already on screen at frame 0, rising into place over the first 10 frames.
        const enter = interpolate(frame, [0, 10], [90, 0], {
          easing: Easings.easeOutBack, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
        });
        return (
          <Group key={id} x={spot.x} y={spot.y + enter + 24 * (1 - f)} scale={0.95 + 0.05 * f} anchorX={0.5} anchorY={0}>
            <CharacterView ref={CAST[id].view} character={CAST[id].ref} scale={spot.scale} anchorX={0.5} />
          </Group>
        );
      })}
    </>
  );
}

// The speaker's name above the subtitle, on their side of the screen.
export function NameTag() {
  const frame = useCurrentFrame();
  const line = lineAt(frame);
  const id = line.line.speaker;
  const spot = SPOTS[id];
  const cast = CAST[id];
  const pop = interpolate(frame, [line.from - line.leadInFrames, line.from + 5], [0, 1], {
    easing: Easings.easeOutBack, extrapolateLeft: 'clamp', extrapolateRight: 'clamp',
  });
  const width = 300;
  const x = spot.tagAlign === 0 ? spot.tagX : spot.tagX - width;
  return (
    <Group x={x + width / 2} y={TAG_Y} anchorX={0.5} anchorY={0.5} scale={0.8 + 0.2 * pop} opacity={pop}>
      <Rect x={-width / 2} y={-34} width={width} height={68} cornerRadius={34} fill={cast.color} stroke={C.paper} strokeWidth={6} />
      <Label x={0} y={0} ax={0.5} ay={0.5} size={38} weight={900} color={C.paper}>{cast.displayName}</Label>
    </Group>
  );
}
