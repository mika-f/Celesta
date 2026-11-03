import { TypewriterText } from '@celesta.studio/components';
import { noise, random } from '@celesta.studio/math';
import { Easings, Rect, progress, useBeat, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import { Camera } from '@celesta.studio/layout';
import { TextReveal } from '@celesta.studio/text';
import { Label, textStyle } from '../components/Label';
import { BEAT, BPM, C, H, W } from '../constants';
import { TOTAL_COMMITS } from '../data';

const NEXT = 'next: higher-level components & hooks';

export function Outro() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const { pulse } = useBeat({ bpm: BPM });

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Camera zoom={1 + 0.06 * (f / durationInFrames)}>
        {Array.from({ length: 70 }, (_, i) => {
          const x = random(`star-${i}-x`) * W + noise(i, f / 40) * 40;
          const y = (((random(`star-${i}-y`) * H - f * (0.6 + random(`star-${i}-v`))) % H) + H) % H;
          const s = 2 + random(`star-${i}-s`) * 4;
          return <Rect key={i} x={x} y={y} width={s} height={s} cornerRadius={s / 2} fill={i % 5 === 0 ? C.mint : C.paper}
            opacity={0.15 + 0.35 * random(`star-${i}-o`) + (i % 4 === 0 ? 0.3 * pulse : 0)} />;
        })}
        <TextReveal x={W / 2} y={380} lineHeight={230} baseline={0.84} align={0.5} from={BEAT * 4}
          style={textStyle('display', 230)}>
          Celesta
        </TextReveal>
        <Label x={W / 2} y={700} size={24} font="mono" color={C.grey} ax={0.5} ay={0.5}
          opacity={progress(f, BEAT * 5, 12)}>{`36 DAYS  ·  ${TOTAL_COMMITS} COMMITS  ·  1 REPOSITORY`}</Label>
        {/* Centered on the finished line, so it does not shift while it types. */}
        <TypewriterText x={W / 2} y={800} anchorX={0.5} anchorY={0.5} from={8} framesPerChar={1 / 1.2}
          blinkFrames={BEAT} style={textStyle('mono', 36, C.mint)} caretShape="block">
          {NEXT}
        </TypewriterText>
      </Camera>
      <Rect width={W} height={H} fill="#000000" opacity={progress(f, durationInFrames - 24, 24, Easings.easeInCubic)} />
    </>
  );
}
