import { Easings, Group, Rect, Sequence, Text, progress, useCurrentFrame } from '@celesta/react';
import { Circle } from '@celesta/shapes';
import { TransitionSeries } from '@celesta/transitions';
import { useAccent, useBarLoop, useStage } from '../components/DemoClock';
import { BAR, C, DEMO, FONT, W } from '../constants';

// <TransitionSeries>: a real one, Sun → wipe → Moon, replayed every bar on a
// full-width screen, under a timeline showing where the two scenes overlap.
// Stages (variants/transition.json): 0 `<T>`, 1 `<Sun />`, 2 the transition, 3 `<Moon />`.
export const TRANSITION_STAGES = 4;

// Keep these equal to the snippet in variants/transition.json. 35 + 35 - 10 = one bar.
const SCENE = 35;
const OVERLAP = 10;

// The screen is as wide as the composition: TransitionSeries moves its wipe
// edge across the composition's width.
const SCREEN = { y: 236, height: Math.round((W * 9) / 16) } as const;
const RULER = { x: DEMO.safe.left, y: 72, width: DEMO.safe.right - DEMO.safe.left } as const;

// Each scene's name sits top-left, where the platforms' buttons never reach.
function SceneName({ children, color }: { children: string; color: string }) {
  return (
    <Text x={DEMO.safe.left} y={44}
      style={{ fontFamily: FONT.display, fontSize: 64, fontWeight: 800, fill: { type: 'solid', color } }}>
      {children}
    </Text>
  );
}

function Sun() {
  const frame = useCurrentFrame();
  return (
    <>
      <Rect width={W} height={SCREEN.height}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: SCREEN.height },
          stops: [{ offset: 0, color: '#FFC266' }, { offset: 1, color: '#FF7A59' }] }} />
      <Circle x={W / 2} y={SCREEN.height / 2 + 60 - frame * 1.5} anchorX={0.5} anchorY={0.5} radius={130} fill="#FFF4D6"
        glow={{ color: '#FFF4D6', blur: 40 }} />
      <Rect y={SCREEN.height - 120} width={W} height={120} fill="#E8603F" />
      <SceneName color="#5A1E12">Sun</SceneName>
    </>
  );
}

function Moon() {
  const frame = useCurrentFrame();
  const stars = [[160, 160], [300, 330], [820, 140], [960, 360], [700, 240], [440, 110], [560, 420]];
  return (
    <>
      <Rect width={W} height={SCREEN.height}
        fill={{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: SCREEN.height },
          stops: [{ offset: 0, color: '#0B1236' }, { offset: 1, color: '#27306B' }] }} />
      {stars.map(([x, y], i) => (
        <Circle key={i} x={x} y={y} anchorX={0.5} anchorY={0.5} radius={6} fill="#FFFFFF"
          opacity={0.4 + 0.6 * Math.abs(Math.sin((frame + i * 7) / 6))} />
      ))}
      <Group x={W / 2} y={SCREEN.height / 2 + 40 - frame * 0.8}>
        <Circle anchorX={0.5} anchorY={0.5} radius={120} fill="#F3F1EC" glow={{ color: '#9DB0FF', blur: 30 }} />
        <Circle x={56} y={-34} anchorX={0.5} anchorY={0.5} radius={108} fill="#141C4A" />
      </Group>
      <Rect y={SCREEN.height - 120} width={W} height={120} fill="#0E1540" />
      <SceneName color="#F3F1EC">Moon</SceneName>
    </>
  );
}

// The series itself, exactly as in the snippet.
function Series() {
  const T = TransitionSeries;
  return (
    <T>
      <T.Sequence durationInFrames={SCENE}><Sun /></T.Sequence>
      <T.Transition type="wipe" durationInFrames={OVERLAP} />
      <T.Sequence durationInFrames={SCENE}><Moon /></T.Sequence>
    </T>
  );
}

function Screen() {
  const local = useCurrentFrame();
  const enter = progress(useStage(0), 0, 12, Easings.easeOutCubic);
  const sun = progress(useStage(1), 0, 10);
  const loop = useBarLoop(3);
  return (
    <Group y={SCREEN.y + 40 * (1 - enter)} opacity={enter} clip={{ width: W, height: SCREEN.height }}>
      <Rect width={W} height={SCREEN.height} fill="#05060A" />
      {loop ? (
        // Restart the series on every downbeat.
        <Sequence key={loop.bar} from={local - loop.t} durationInFrames={BAR}><Series /></Sequence>
      ) : (
        <Group opacity={sun}><Sun /></Group>
      )}
    </Group>
  );
}

// Two clips on a one-bar ruler: where they overlap is the transition.
function Timeline({ accent }: { accent: string }) {
  const enter = progress(useStage(0), 0, 12, Easings.easeOutCubic);
  const sun = progress(useStage(1), 0, 10, Easings.easeOutCubic);
  const overlap = progress(useStage(2), 0, 10, Easings.easeOutCubic);
  const moon = progress(useStage(3), 0, 10, Easings.easeOutCubic);
  const loop = useBarLoop(3);
  const px = RULER.width / BAR;
  const label = (x: number, text: string, color: string) => (
    <Text x={x} y={24} anchorY={0.5}
      style={{ fontFamily: FONT.mono, fontSize: 28, fontWeight: 700, fill: { type: 'solid', color } }}>
      {text}
    </Text>
  );
  return (
    <Group x={RULER.x} y={RULER.y} opacity={enter}>
      <Rect width={RULER.width} height={104} cornerRadius={10} fill="#0B0D15" />
      <Group y={4} opacity={sun}>
        <Rect width={SCENE * px * sun} height={46} cornerRadius={8} fill="#FFB35C" />
        {label(16, 'Sun', '#5A1E12')}
      </Group>
      <Group y={54} opacity={moon}>
        <Rect x={(SCENE - OVERLAP) * px} width={SCENE * px * moon} height={46} cornerRadius={8} fill="#3E4CA8" />
        {label((SCENE - OVERLAP) * px + 16, 'Moon', '#F3F1EC')}
      </Group>
      <Group x={(SCENE - OVERLAP) * px} opacity={overlap}>
        <Rect y={-8} width={OVERLAP * px} height={120} cornerRadius={6} fill={`${accent}40`} stroke={accent} strokeWidth={3} />
        <Text x={(OVERLAP * px) / 2} y={-34} anchorX={0.5} anchorY={0.5}
          style={{ fontFamily: FONT.mono, fontSize: 28, fontWeight: 700, fill: { type: 'solid', color: accent } }}>
          {`wipe ${OVERLAP}f`}
        </Text>
      </Group>
      {loop && <Rect x={loop.t * px - 2} y={-12} width={4} height={128} fill={C.paper} />}
    </Group>
  );
}

export function TransitionDemo() {
  const accent = useAccent();
  return (
    <>
      <Timeline accent={accent} />
      <Screen />
    </>
  );
}
