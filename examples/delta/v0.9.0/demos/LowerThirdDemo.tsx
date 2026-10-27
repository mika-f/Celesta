import { Group, Rect, Sequence } from '@celesta/react';
import { LowerThird, PatternBackground } from '@celesta/components';
import type { LowerThirdProps } from '@celesta/components';
import { LayoutBounds } from '@celesta/layout';
import { Delta } from '../../components/Delta';
import { C } from '../../components/theme';
import type { DemoProps } from '../program';
import { STAGE } from '../program';
import { BAR } from '../timeline';
import { Note } from './common';

// LOWER THIRD: four real <LowerThird>s, one every two beats, in the
// monitor's own area (<LayoutBounds>): bottom-left with a role, bottom-right
// on a darker band, top-left, and a long Japanese name that wraps and
// shrinks to its two lines.

/** The Japanese the demo draws. */
export const LOWER_THIRD_STRINGS = ['しずく', 'ナレーター', '長い名前でも二行に収まるまで小さくなります', 'デルタ'];

const shared: Partial<LowerThirdProps> = {
  fontFamily: 'sans', color: 'paper', subtitleColor: 'grey', background: '#0C0D0FD9', cornerRadius: 4,
  titleSize: 40, subtitleSize: 20, margin: 30,
};

const TAKES: { props: LowerThirdProps; note: string }[] = [
  { props: { title: 'しずく', subtitle: 'ナレーター · voice-folder/shizuku', accent: 'orange' }, note: 'placement="bottom-left"' },
  { props: { title: 'DELTA 0.9.0', subtitle: 'v0.8.0 → main', placement: 'bottom-right', accent: 'blue', background: '#0C0D0F' },
    note: 'placement="bottom-right"' },
  { props: { title: 'ON AIR', subtitle: '100 BPM · 30 fps', placement: 'top-left', accent: 'tally' }, note: 'placement="top-left"' },
  { props: { title: LOWER_THIRD_STRINGS[2], subtitle: 'maxTitleLines={2}', accent: 'amber', maxWidth: 520, minTitleSize: 22 },
    note: 'wraps, then shrinks to fit' },
];

/** The camera feed under the captions: a turning Δ on the studio floor. */
function Feed({ frame }: { frame: number }) {
  return (
    <Group>
      <PatternBackground width={STAGE.width} height={STAGE.height} pattern="grid" spacing={50} size={1}
        background="panel" color="paper" opacity={0.06} />
      <Rect y={380} width={STAGE.width} height={STAGE.height - 380} fill="#0C0D0F" opacity={0.5} />
      <Group x={STAGE.width / 2} y={250} rotationY={frame * 2.5} perspective={1200}>
        <Delta size={230} fill={C.orange} />
      </Group>
      <Rect x={STAGE.width / 2 - 150} y={392} width={300} height={10} cornerRadius={5} fill="#000000" opacity={0.35} />
    </Group>
  );
}

export function LowerThirdDemo({ frame }: DemoProps) {
  const take = Math.min(TAKES.length - 1, Math.max(0, Math.floor(frame / BAR)));
  return (
    <Group>
      <Feed frame={frame} />
      <LayoutBounds width={STAGE.width} height={STAGE.height}>
        {TAKES.map((t, i) => (
          <Sequence key={i} from={i * BAR} durationInFrames={BAR}>
            <LowerThird {...shared} {...t.props} />
          </Sequence>
        ))}
      </LayoutBounds>
      <Note x={STAGE.width - 24} y={24} anchorX={1}>{TAKES[take].note}</Note>
    </Group>
  );
}
