// The Components scene's data: twelve cards drawn from one array, which is
// also the array shown in the code panel (test/cards.test.ts runs that text).

export type CardDatum = { title: string; value: number };

const BASE: CardDatum[] = [
  { title: 'Tokyo', value: 72 },
  { title: 'Osaka', value: 48 },
  { title: 'Seoul', value: 64 },
  { title: 'Taipei', value: 36 },
  { title: 'Berlin', value: 58 },
  { title: 'Paris', value: 44 },
  { title: 'Lisbon', value: 30 },
  { title: 'Oslo', value: 22 },
  { title: 'Austin', value: 66 },
  { title: 'Lima', value: 40 },
  { title: 'Cairo', value: 52 },
  { title: 'Perth', value: 28 },
];

// One card's value changes every two beats from local frame 120.
export const CHANGES = [
  { at: 120, index: 3, value: 91 },
  { at: 150, index: 7, value: 84 },
  { at: 180, index: 0, value: 18 },
  { at: 210, index: 10, value: 97 },
];

export function cardsAt(local: number): CardDatum[] {
  const data = BASE.map((d) => ({ ...d }));
  for (const { at, index, value } of CHANGES) if (local >= at) data[index].value = value;
  return data;
}

export function cardsSource(data: CardDatum[]): string {
  const rows = data.map(({ title, value }) => `  { title: '${title}', value: ${value} },`);
  return ['[', ...rows, ']'].join('\n');
}

export const CARD_SOURCE = `function Card({ x, y, title, value }) {
  return (
    <Group x={x} y={y}>
      <Text>{title}</Text>
      <Text y={40}>{value}</Text>
      <Rect y={120} width={value * 2.2} height={6} />
    </Group>
  );
}`;
