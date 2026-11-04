import { Group, Rect, Sequence, Span } from '@celesta.studio/react';
import { LayoutBounds } from '@celesta.studio/layout';
import { Motion, PatternBackground, TitleCard, TypewriterText } from '@celesta.studio/components';
import type { DemoProps } from '../../components/ChapterShell';
import { CUT, STAGE } from '../../components/ChapterShell';
import { C } from '../../components/theme';
import { BAR, BEAT } from '../timeline';

// Above: a TitleCard laid out in a 860×380 area (LayoutBounds), its pieces
// coming in one after another. Below: a TypewriterText whose caret sits on
// the measured layout, and three lines that Motion brings in, holds and
// takes out on the chapter's clock.
const AREA = { width: STAGE.width, height: 380 };
const LINES = [
  { text: 'enter="fade-up"', enter: 'fade-up' },
  { text: "enter={{ type: 'fade-left', distance: 40 }}", enter: { type: 'fade-left', distance: 40 } },
  { text: 'enter="scale"  exit="fade-down"', enter: 'scale' },
] as const;

export function TitlesDemo(_: DemoProps) {
  return (
    <Group>
      <Sequence from={CUT}>
        <Group clip={{ width: AREA.width, height: AREA.height, cornerRadius: 10 }}>
          <LayoutBounds width={AREA.width} height={AREA.height}>
            <TitleCard chapter="04" kicker="NEW IN 0.8.0" kickerRule title={'Code is\nthe title'}
              subtitle="TitleCard, Motion, TypewriterText" fontFamily="sans" titleSize={70} subtitleSize={22} kickerSize={15}
              background={<PatternBackground width={AREA.width} height={AREA.height} pattern="stripes" angle={-30}
                spacing={28} size={10} background="orange" color="ink" opacity={0.08} speedX={20} />}
              color="ink" subtitleColor="ink" accentColor="paper" />
          </LayoutBounds>
        </Group>
        <Rect y={410} width={STAGE.width} height={82} cornerRadius={10} fill="#08090B" />
        <TypewriterText x={24} y={432} from={BEAT} framesPerChar={1.2} caretShape="block" caretColor="orange"
          fontFamily="mono" fontSize={30} color="paper">
          <Span color="orange" fontWeight={700}>$</Span> pnpm add @celesta.studio/components
        </TypewriterText>
        {LINES.map((line, i) => (
          <Motion key={line.text} from={BAR + i * BEAT} durationInFrames={3 * BAR - i * BEAT - 6}
            enter={line.enter} exit={i === 2 ? 'fade-down' : 'fade'}>
            <Rect x={0} y={530 + i * 66} width={8} height={40} fill={i === 0 ? C.orange : C.grey} />
            {/* Motion's children run on its own clock: 0 is the moment it enters. */}
            <TypewriterText x={28} y={536 + i * 66} framesPerChar={0.5} caretShape="line"
              fontFamily="mono" fontSize={24} color="paper">{`<Motion ${line.text}>`}</TypewriterText>
          </Motion>
        ))}
      </Sequence>
    </Group>
  );
}
