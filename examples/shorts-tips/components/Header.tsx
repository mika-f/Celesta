import { Group, Rect, Text } from '@celesta/react';
import { C, FONT, HEADER, SAFE, SAFE_W } from '../constants';

// The badge and the Tip's title, on screen from the first frame to the last.
export function Header({ number, title, accent }: { number: number; title: string; accent: string }) {
  const badge = `TIPS #${String(number).padStart(2, '0')}`;
  return (
    <Group x={SAFE.left} y={HEADER.y}>
      <Rect width={222} height={52} cornerRadius={26} fill={accent} />
      <Text x={111} y={26} anchorX={0.5} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 24, fontWeight: 800, fill: { type: 'solid', color: C.ink } }}>
        {badge}
      </Text>
      <Text x={246} y={26} anchorY={0.5}
        style={{ fontFamily: FONT.display, fontSize: 24, fontWeight: 800, letterSpacing: 4, fill: { type: 'solid', color: C.grey } }}>
        CELESTA
      </Text>
      <Text y={HEADER.titleY - HEADER.y} maxWidth={SAFE_W}
        style={{
          fontFamily: FONT.ja, fontSize: HEADER.titleSize, fontWeight: 900, lineHeight: HEADER.titleLine,
          lineBreak: 'phrase', fill: { type: 'solid', color: C.paper },
        }}>
        {title}
      </Text>
    </Group>
  );
}
