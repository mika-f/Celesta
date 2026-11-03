import { useContext } from 'react';
import { Easings, FreezeFrame, Group, Mask, Rect, Text, interpolate, progress } from '@celesta.studio/react';
import { Circle } from '@celesta.studio/shapes';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, H, W, solid } from '../../components/theme';
import { FilmContext } from '../film-context';
import { BAR, OPEN_LENGTH } from '../timeline';

// Two bars of each. First the film's own DELTA card plays again through the
// letters of MASK (an alpha mask). Then a blurred circle is a luminance
// mask over the DELTA card in full colour, with a gray copy beneath; on the
// last bar it inverts.
/** The film scaled to cover the stage's height. */
const COVER = STAGE.height / H;

function Label({ text, on }: { text: string; on: number }) {
  return (
    <Group opacity={on}>
      <Rect x={24} y={24} width={text.length * 13.2 + 32} height={42} cornerRadius={21} fill="#0C0D0FCC" />
      <Text x={40} y={45} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 22, fill: solid(C.paper) }}>{text}</Text>
    </Group>
  );
}

function OpenPlaying({ frame }: { frame: number }) {
  const Film = useContext(FilmContext);
  return (
    <Group scale={COVER} x={-(W * COVER - STAGE.width) / 2}>
      <FreezeFrame frame={Math.max(0, Math.min(OPEN_LENGTH - 1, OPEN_LENGTH - 2 * BAR + frame))}><Film /></FreezeFrame>
    </Group>
  );
}

export function MaskDemo({ frame }: DemoProps) {
  const Film = useContext(FilmContext);
  const second = frame >= 2 * BAR;
  if (!second) {
    const grow = interpolate(frame, [0, 2 * BAR], [0.9, 1.1], { easing: Easings.easeInOutSine, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
    const word = { fontFamily: FONT.display, fontSize: 420, letterSpacing: { em: 0.01 } } as const;
    return (
      <Group>
        <Rect width={STAGE.width} height={STAGE.height} fill="#0A0B0D" />
        <Group x={STAGE.width / 2} y={STAGE.height / 2 + 20} scale={grow}>
          <Text anchorX={0.5} anchorY={0.5} style={{ ...word, fill: solid('#00000000'), stroke: { paint: solid(C.line), width: 3 } }}>MASK</Text>
        </Group>
        <Group mask={
          <Group x={STAGE.width / 2} y={STAGE.height / 2 + 20} scale={grow}>
            <Text anchorX={0.5} anchorY={0.5} style={{ ...word, fill: solid('#FFFFFF') }}>MASK</Text>
          </Group>
        }>
          <OpenPlaying frame={frame} />
        </Group>
        <Label text={'mask={<Text>MASK</Text>}'} on={enter(frame, 6)} />
      </Group>
    );
  }
  const f = frame - 2 * BAR;
  const invert = f >= BAR;
  const t = f / (2 * BAR);
  const spot = { x: STAGE.width * (0.3 + 0.4 * t), y: STAGE.height * (0.5 + 0.18 * Math.sin(t * 6)) };
  const r = interpolate(f, [0, 12], [40, 210], { easing: Easings.easeOutBack, extrapolateRight: 'clamp' });
  const card = (
    <Group scale={COVER} x={-(W * COVER - STAGE.width) / 2}>
      <FreezeFrame frame={190}><Film /></FreezeFrame>
    </Group>
  );
  return (
    <Group>
      <Group grayscale={1} brightness={0.45}>{card}</Group>
      <Group>
        <Mask mode="luminance" invert={invert}>
          <Circle x={spot.x} y={spot.y} anchorX={0.5} anchorY={0.5} radius={r} fill="#FFFFFF" blur={36} />
        </Mask>
        {card}
      </Group>
      <Label text={invert ? '<Mask mode="luminance" invert>' : '<Mask mode="luminance">'} on={enter(f, 2)} />
      <Text x={STAGE.width - 24} y={STAGE.height - 24} anchorX={1} anchorY={1} opacity={progress(f, 4, 10)}
        style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.grey) }}>{'<Circle blur={36} />'}</Text>
    </Group>
  );
}
