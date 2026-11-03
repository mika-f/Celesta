import { useContext } from 'react';
import { FreezeFrame, Group, Rect, Text } from '@celesta.studio/react';
import type { DemoProps } from '../../components/ChapterShell';
import { STAGE, enter } from '../../components/ChapterShell';
import { C, FONT, FPS, H, W, solid } from '../../components/theme';
import { FilmContext } from '../film-context';
import { BEAT, DURATION, chapterAt } from '../timeline';

// An agent's session, drawn: it asks the Celesta MCP server to render three
// frames of this film and gets them back as images (here they are those
// frames, redrawn by <FreezeFrame>), then inspects the entry.
const FRAMES = [chapterAt(8) + 60, chapterAt(5) + 90, chapterAt(3) + 50];
const mono = (size: number, color: string, weight = 400) =>
  ({ fontFamily: FONT.mono, fontSize: size, fontWeight: weight, fill: solid(color) }) as const;

function Call({ name, args, at, frame }: { name: string; args: string; at: number; frame: number }) {
  const p = enter(frame, at, 10);
  const typed = Math.max(0, Math.floor((frame - at) * 4));
  return (
    <Group opacity={p}>
      <Rect width={STAGE.width} height={74} cornerRadius={8} fill={C.raised} />
      <Rect width={5} height={74} fill={C.orange} />
      <Text x={22} y={24} anchorY={0.5} style={mono(19, C.orange, 700)}>{`celesta · ${name}`}</Text>
      <Text x={22} y={54} anchorY={0.5} style={{ ...mono(17, C.paper), visibleCharacters: typed }}>{args}</Text>
    </Group>
  );
}

export function McpDemo({ frame }: DemoProps) {
  const Film = useContext(FilmContext);
  const thumb = { width: 268, height: 151 };
  const scale = thumb.width / W;
  // The fields of inspect's result that say which composition and how long;
  // `compositions` lists every root with its size and length (shortened here).
  const inspect = `{ "compositionId": "film", "composition": { "width": ${W}, "height": ${H}, "fps": ${FPS}, "frames": ${DURATION} },`
    + ' "compositions": [{ "id": "film", … }, { "id": "poster", … }] }';
  return (
    <Group>
      <Group opacity={enter(frame, 0)}>
        <Text style={mono(19, C.grey)}>{'> render the spectrum, blend and effects'}</Text>
        <Text y={30} style={mono(19, C.grey)}>{'  chapters of film-0.7.0.tsx and check them'}</Text>
      </Group>
      <Group y={80}>
        <Call name="render_frames" args={`{ "source": "film-0.7.0.tsx", "frames": [${FRAMES.join(', ')}] }`} at={BEAT} frame={frame} />
      </Group>
      <Group y={176}>
        {FRAMES.map((at, i) => {
          const p = enter(frame, 3 * BEAT + i * 5, 12);
          return (
            <Group key={at} x={i * (thumb.width + 28)} opacity={p} y={16 * (1 - p)}>
              <Rect x={-2} y={-2} width={thumb.width + 4} height={thumb.height + 4} cornerRadius={6} fill={C.line} />
              <Group clip={{ width: thumb.width, height: thumb.height, cornerRadius: 4 }}>
                <Group scale={scale}><FreezeFrame frame={at}><Film /></FreezeFrame></Group>
              </Group>
              <Text y={thumb.height + 24} anchorY={0.5} style={mono(16, C.grey)}>{`image/png · frame ${at}`}</Text>
            </Group>
          );
        })}
      </Group>
      <Group y={392}>
        <Call name="inspect" args={'{ "entry": "film-0.7.0.tsx" }'} at={8 * BEAT} frame={frame} />
      </Group>
      <Group y={482} opacity={enter(frame, 10 * BEAT)}>
        <Text maxWidth={STAGE.width} style={{ ...mono(17, C.green), lineHeight: 26 }}>{inspect}</Text>
      </Group>
      <Group y={560} opacity={enter(frame, 12 * BEAT)}>
        <Text style={mono(17, C.dim)}>tools</Text>
        {['inspect', 'render_frames', 'contact_sheet', 'export_video', 'init_project', 'list_templates', 'read_docs'].map((tool, i) => (
          <Group key={tool} x={(i % 4) * 214} y={30 + Math.floor(i / 4) * 52}>
            <Rect width={202} height={40} cornerRadius={20} fill={C.raised} stroke={C.line} strokeWidth={2} />
            <Text x={101} y={20} anchorX={0.5} anchorY={0.5} style={mono(16, C.paper)}>{tool}</Text>
          </Group>
        ))}
      </Group>
    </Group>
  );
}
