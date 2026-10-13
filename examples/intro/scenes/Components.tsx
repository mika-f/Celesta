import { Easings, interpolate, useCurrentFrame } from '@celesta/react';
import { Card } from '../components/Card';
import { CodePanel } from '../components/CodePanel';
import { Headline } from '../components/Headline';
import { BAR, BEAT } from '../constants';
import { CARD_SOURCE, CHANGES, cardsAt, cardsSource } from '../shared/cards';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

const GRID = { x: 920, y: 200, columns: 3, dx: 300, dy: 180 } as const;
// CARD_SOURCE's lines, a blank line, then `const cities = [` and one row per card.
const FIRST_ROW = CARD_SOURCE.split('\n').length + 3;

// 26–34 s. One component, twelve cards from one array. Edits to the array
// show up in the matching card on the same frame.
export function Components() {
  const local = useCurrentFrame();
  const data = cardsAt(local);
  const source = `${CARD_SOURCE}\n\nconst cities = ${cardsSource(data)};`;
  const latest = [...CHANGES].reverse().find((c) => local >= c.at && local < c.at + BEAT);
  return (
    <>
      <Headline x={80} y={96} size={80}>{local >= BAR * 2 ? 'Feed it data.' : 'Write it once.'}</Headline>
      <CodePanel x={80} y={200} width={760} height={780} source={source} fontSize={18} title="cards.tsx"
        highlightLines={latest ? [FIRST_ROW + latest.index] : undefined} />
      {data.map((datum, i) => {
        const enter = interpolate(local, [i * 4, i * 4 + 12], [0, 1], { ...clamp, easing: Easings.easeOutCubic });
        const change = CHANGES.find((c) => c.index === i && local >= c.at);
        const from = cardsAt(change ? change.at - 1 : 0)[i].value;
        const bar = change ? interpolate(local, [change.at, change.at + 8], [from, datum.value], { ...clamp, easing: Easings.easeOutCubic }) : datum.value;
        const x = GRID.x + (i % GRID.columns) * GRID.dx;
        const y = GRID.y + Math.floor(i / GRID.columns) * GRID.dy + 20 * (1 - enter);
        return <Card key={datum.title} x={x} y={y} datum={datum} bar={bar} lit={latest?.index === i} opacity={enter} />;
      })}
    </>
  );
}
