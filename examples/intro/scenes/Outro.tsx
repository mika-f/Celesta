import { Easings, Group, Rect, interpolate, spring, useCurrentFrame, useTextMetrics, useVideoConfig } from '@celesta/react';
import { Headline } from '../components/Headline';
import { Label } from '../components/Label';
import { C, FONT } from '../constants';
import { HOLD, S } from '../shared/cues';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;
const URL = 'github.com/mika-f/celesta';
const URL_SIZE = 36;
// Every animation finishes before the end-screen hold.
const END = HOLD - S.outro;

function useEnter(at: number) {
  const local = useCurrentFrame();
  return interpolate(local, [at, at + 12], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
}

// 52–60 s. The wordmark, the tagline, and where to get it, on the left half;
// the right half stays empty for YouTube's end-screen elements. Still from
// 55 s.
export function Outro() {
  const local = Math.min(useCurrentFrame(), END - 1);
  const { fps } = useVideoConfig();
  const mark = spring({ frame: local, fps, config: { stiffness: 160, damping: 22 } });
  const tagline = useEnter(30);
  const detail = useEnter(45);
  const url = useEnter(60);
  const underline = interpolate(local, [62, 82], [0, 1], { ...clamp, easing: Easings.easeInOutCubic });
  const { width } = useTextMetrics(URL, { fontFamily: FONT.mono, fontSize: URL_SIZE });
  return (
    <>
      <Group y={40 * (1 - mark)} opacity={mark}>
        <Headline x={160} y={360} size={200}>CELESTA</Headline>
      </Group>
      <Headline x={160} y={620} size={64} opacity={tagline}>Video, written in code.</Headline>
      <Label x={160} y={720} size={28} color={C.muted} opacity={detail}>React · TypeScript · macOS / Windows</Label>
      <Label x={160} y={800} size={URL_SIZE} color={C.accent} opacity={url}>{URL}</Label>
      <Rect x={160} y={800 + URL_SIZE * 1.4} width={width * underline} height={1} fill={C.accent} />
    </>
  );
}
