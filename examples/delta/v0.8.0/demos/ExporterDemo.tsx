import { useContext } from 'react';
import { Easings, FreezeFrame, Group, Rect, Text, interpolate } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, FPS, H, W, solid } from '../../components/theme';
import { FilmContext } from '../film-context';
import { BAR, BEAT, DURATION, chapterAt } from '../timeline';

// A Node script exporting this film with @celesta.studio/exporter, drawn: the
// onProgress events as they arrive (their names and fields are the
// package's; the numbers are this film's), the bar, and what it wrote. The
// stills are a frame of this film, redrawn by <FreezeFrame>: one of Δ01,
// which replays nothing itself (the outro would replay this chapter).
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;
const RENDER = { from: BEAT, to: 3 * BAR };
const STILL_AT = chapterAt(0) + 100;

const EVENTS: readonly { at: number; name: string; detail: string }[] = [
  { at: 2, name: 'compositions', detail: 'film, poster' },
  { at: 6, name: 'composition', detail: `${W}×${H} · ${FPS} fps · ${DURATION} frames` },
  { at: RENDER.from, name: 'rendering', detail: '' },
  { at: RENDER.to + 2, name: 'mixingAudio', detail: '' },
  { at: RENDER.to + 6, name: 'muxing', detail: '' },
  { at: 3 * BAR + 2 * BEAT, name: 'wrote', detail: 'delta-0.8.0.mp4' },
  { at: 3 * BAR + 2 * BEAT + 4, name: 'wrote', detail: 'poster-0.8.0.jpg' },
  { at: 3 * BAR + 2 * BEAT + 8, name: 'wrote', detail: 'poster-0.8.0.webp' },
];

export function ExporterDemo({ frame }: DemoProps) {
  const Film = useContext(FilmContext);
  const done = interpolate(frame, [RENDER.from, RENDER.to], [0, 1], { easing: Easings.easeInOutSine, extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const rendered = Math.max(1, Math.round(done * DURATION));
  const thumb = { width: 260, height: 146 };
  return (
    <Group>
      <Group opacity={enter(frame, 0)}>
        <Rect width={STAGE.width} height={420} cornerRadius={10} fill="#08090B" stroke={C.line} strokeWidth={2} />
        <Text x={24} y={34} anchorY={0.5} style={mono(19, C.grey)}>{'$ node export.mjs'}</Text>
        {EVENTS.map((event, i) => {
          if (frame < event.at) return null;
          const detail = event.name === 'rendering' ? `frame ${rendered} / ${DURATION}` : event.detail;
          return (
            <Group key={i} y={78 + i * 40}>
              <Text x={24} anchorY={0.5} style={mono(19, event.name === 'wrote' ? C.green : C.orange, 700)}>{event.name}</Text>
              <Text x={230} anchorY={0.5} style={mono(19, C.paper)}>{detail}</Text>
            </Group>
          );
        })}
      </Group>
      <Group y={446} opacity={enter(frame, RENDER.from)}>
        <Rect width={STAGE.width} height={14} cornerRadius={7} fill={C.raised} />
        <Rect width={STAGE.width * done} height={14} cornerRadius={7} fill={done >= 1 ? C.green : C.orange} />
        <Text x={STAGE.width} y={40} anchorX={1} anchorY={0.5} style={mono(18, C.grey)}>{`${Math.round(done * 100)}%`}</Text>
        <Text y={40} anchorY={0.5} style={mono(18, C.grey)}>{"{ event: 'rendering', frame, total }"}</Text>
      </Group>
      <Group y={540}>
        {['MP4', 'JPEG · quality 85', 'WebP'].map((label, i) => {
          const p = enter(frame, EVENTS[5 + i].at, 12);
          if (p <= 0) return null;
          return (
            <Group key={label} x={i * (thumb.width + 40)} opacity={p} y={16 * (1 - p)}>
              <Rect x={-2} y={-2} width={thumb.width + 4} height={thumb.height + 4} cornerRadius={6} fill={i === 0 ? C.orange : C.line} />
              <Group clip={{ width: thumb.width, height: thumb.height, cornerRadius: 4 }}>
                <Group scale={thumb.width / W}><FreezeFrame frame={STILL_AT}><Film /></FreezeFrame></Group>
              </Group>
              <Text y={thumb.height + 24} anchorY={0.5} style={mono(16, C.grey)}>{label}</Text>
            </Group>
          );
        })}
      </Group>
    </Group>
  );
}
