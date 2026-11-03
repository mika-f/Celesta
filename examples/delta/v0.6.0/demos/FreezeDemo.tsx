import { useContext } from 'react';
import { Easings, FreezeFrame, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import { Delta } from '../../components/Delta';
import { STAGE, enter } from '../../components/ChapterShell';
import type { DemoProps } from '../../components/ChapterShell';
import { C, FONT, H, W, solid } from '../../components/theme';
import { FilmContext } from '../film-context';
import { chapterAt } from '../timeline';

// The player rewinds this very film to Δ01 and plays it again, and the strip
// holds five earlier moments: all of it is <FreezeFrame> around the film.
const PLAYER = { x: 30, y: 26, width: 800 };
const THUMB = { width: 150, gap: 12 };

export function FreezeDemo({ frame }: DemoProps) {
  const Film = useContext(FilmContext);
  // Back from the middle of Δ05 to the start of Δ01, then play from there.
  const rewound = interpolate(frame, [0, 36], [chapterAt(4) + 60, chapterAt(0) + 12],
    { easing: Easings.easeInOutCubic, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const at = Math.round(frame < 36 ? rewound : chapterAt(0) + 12 + (frame - 36));
  const rewinding = frame < 36;
  const mono = { fontFamily: FONT.mono, fontSize: 18 };
  const scale = PLAYER.width / W;
  return (
    <Group>
      <Rect width={STAGE.width} height={STAGE.height} fill={C.ink} />
      <Group x={PLAYER.x} y={PLAYER.y} clip={{ width: PLAYER.width, height: H * scale, cornerRadius: 8 }}>
        <Group scale={scale}><FreezeFrame frame={at}><Film /></FreezeFrame></Group>
        <Rect y={H * scale - 44} width={PLAYER.width} height={44} fill="#0C0D0FCC" />
        {rewinding
          ? <Group x={22} y={H * scale - 22}>
              <Delta x={6} size={16} rotation={-90} fill={C.orange} />
              <Delta x={20} size={16} rotation={-90} fill={C.orange} />
            </Group>
          : <Delta x={24} y={H * scale - 22} size={18} rotation={90} fill={C.paper} />}
        <Text x={50} y={H * scale - 22} anchorY={0.5} style={{ ...mono, fontWeight: 700, fill: solid(rewinding ? C.orange : C.paper) }}>
          {`${rewinding ? 'REWIND' : 'PLAY'}   <FreezeFrame frame={${at}}>`}
        </Text>
      </Group>
      {[0, 1, 2, 3, 4].map((chapter, i) => {
        const frozen = chapterAt(chapter) + 70;
        const p = enter(frame, 10 + i * 4, 16);
        const x = PLAYER.x + i * (THUMB.width + THUMB.gap);
        return (
          <Group key={chapter} x={x} y={520 + 20 * (1 - p)} opacity={p}>
            <Group clip={{ width: THUMB.width, height: THUMB.width * H / W, cornerRadius: 4 }}>
              <Group scale={THUMB.width / W}><FreezeFrame frame={frozen}><Film /></FreezeFrame></Group>
            </Group>
            <Text y={THUMB.width * H / W + 24} anchorY={0.5} style={{ ...mono, fontSize: 16, fill: solid(C.grey) }}>
              {`frame ${frozen}`}
            </Text>
          </Group>
        );
      })}
      <Text x={PLAYER.x} y={STAGE.height - 50} anchorY={0.5} opacity={enter(frame, 30)} style={{ ...mono, fill: solid(C.grey) }}>
        Frozen children are drawn as the film looked at that frame.
      </Text>
    </Group>
  );
}
