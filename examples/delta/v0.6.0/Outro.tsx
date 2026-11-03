import { useContext } from 'react';
import { FreezeFrame, Group, Rect, Span, Text, useCurrentFrame } from '@celesta.studio/react';
import { Backdrop } from '../components/Backdrop';
import { CUT } from '../components/ChapterShell';
import { Delta } from '../components/Delta';
import { C, FONT, H, W, ease, solid } from '../components/theme';
import { FilmContext } from './film-context';
import { TAGLINE } from './Open';

// The mark, the tagline, and every chapter again: each thumbnail is the film
// itself, redrawn at an earlier frame by <FreezeFrame>.
export const OUTRO_LENGTH = 240;
const THUMB = { width: 224, height: 126, gap: 24, columns: 6 };

export function Outro({ chapters }: { chapters: readonly { title: string; at: number }[] }) {
  const Film = useContext(FilmContext);
  const frame = useCurrentFrame() - CUT;
  const mark = ease(frame, -4, 24);
  const title = ease(frame, 4, 24);
  const line = ease(frame, 12, 24);
  const sub = ease(frame, 18, 24);
  const foot = ease(frame, 80, 24);
  const scale = THUMB.width / W;
  const left = (W - (THUMB.columns * THUMB.width + (THUMB.columns - 1) * THUMB.gap)) / 2;
  return (
    <Group>
      <Backdrop />
      <Delta x={W / 2} y={170} size={130} opacity={mark} scale={0.6 + 0.4 * mark}
        glow={{ color: '#F26A2EAA', blur: 36 }} />
      <Text x={W / 2} y={320 + 20 * (1 - title)} anchorX={0.5} anchorY={0.5} opacity={title}
        style={{ fontFamily: FONT.display, fontSize: 170, letterSpacing: { em: 0.04 }, fill: solid(C.paper) }}>
        CELESTA <Span style={{ fill: C.orange }}>0.6.0</Span>
      </Text>
      <Text x={W / 2} y={440} anchorX={0.5} anchorY={0.5} opacity={line}
        style={{ fontFamily: FONT.sans, fontSize: 40, fontWeight: 700, fill: solid(C.paper) }}>
        {TAGLINE}
      </Text>
      <Text x={W / 2} y={494} anchorX={0.5} anchorY={0.5} opacity={sub}
        style={{ fontFamily: FONT.sans, fontSize: 28, fill: solid(C.grey) }}>
        A diff in the code is a diff in the video.
      </Text>
      {chapters.map((chapter, i) => {
        const col = i % THUMB.columns;
        const row = Math.floor(i / THUMB.columns);
        const x = left + col * (THUMB.width + THUMB.gap);
        const y = 560 + row * (THUMB.height + 64);
        const p = ease(frame, 26 + i * 3, 20);
        return (
          <Group key={chapter.title} x={x} y={y + 24 * (1 - p)} opacity={p}>
            <Rect x={-2} y={-2} width={THUMB.width + 4} height={THUMB.height + 4} cornerRadius={6} fill={C.line} />
            <Group clip={{ width: THUMB.width, height: THUMB.height, cornerRadius: 4 }}>
              <Group scale={scale}>
                <FreezeFrame frame={chapter.at}><Film /></FreezeFrame>
              </Group>
            </Group>
            <Text y={THUMB.height + 28} anchorY={0.5} style={{ fontFamily: FONT.mono, fontSize: 17, fontWeight: 700, fill: solid(C.orange) }}>
              {`Δ${String(i + 1).padStart(2, '0')}`}
            </Text>
            <Text x={50} y={THUMB.height + 28} anchorY={0.5} style={{ fontFamily: FONT.display, fontSize: 30, fill: solid(C.paper) }}>
              {chapter.title}
            </Text>
          </Group>
        );
      })}
      <Text x={W / 2} y={H - 60} anchorX={0.5} anchorY={0.5} opacity={foot}
        style={{ fontFamily: FONT.mono, fontSize: 20, fill: solid(C.dim) }}>
        github.com/mika-f/Celesta   ·   examples/delta
      </Text>
    </Group>
  );
}
