import { Group, Rect, Text, useTextMetrics } from '@celesta/react';
import { BAND_HEIGHT, BANDS, CAP_HEIGHT, INK, LEFT, PAPER, RED, TYPE_SIZE, WORD_WIDTH, WORD_Y, CHAPTERS } from './motion';

export function Label({ text, x = LEFT, y, color = PAPER, size = 22 }: {
  text: string; x?: number; y: number; color?: string; size?: number;
}) {
  return <Text x={x} y={y} style={{ fontFamily: 'IBM Plex Mono', fontSize: size,
    fill: { type: 'solid', color } }}>{text}</Text>;
}

export function Word({ text, x = 0, y = 0, width = WORD_WIDTH,
  size = TYPE_SIZE, color = PAPER }: {
  text: string; x?: number; y?: number; width?: number; size?: number; color?: string;
}) {
  const style = { fontFamily: 'Bebas Neue', fontSize: size,
    fill: { type: 'solid' as const, color } };
  const metrics = useTextMetrics(text, style);
  return <Text x={x} y={y} scaleX={width / metrics.width} style={style}>{text}</Text>;
}

export type BandMotion = { x?: number; y?: number; stretch?: number; cover?: number };

/** Actual text pixels, clipped into contiguous strips; never a glyph-morph API. */
export function SlicedWord({ text, x = LEFT, y = WORD_Y, width = WORD_WIDTH,
  size = TYPE_SIZE, color = PAPER, bands = () => ({}) }: {
  text: string; x?: number; y?: number; width?: number; size?: number; color?: string;
  bands?: (index: number) => BandMotion;
}) {
  const strip = BAND_HEIGHT * size / TYPE_SIZE;
  const poses = Array.from({ length: BANDS }, (_, i) => bands(i));
  if (poses.every(p => !p.x && !p.y && !p.cover && (p.stretch === undefined || p.stretch === 1))) {
    return <Word text={text} x={x} y={y} width={width} size={size} color={color} />;
  }
  return <Group x={x} y={y}>
    {poses.map((pose, i) => {
      return <Group key={i} x={pose.x ?? 0} y={i * strip + (pose.y ?? 0)}
        scaleX={pose.stretch ?? 1} clip={{ width, height: strip + 0.7 }}>
        <Word text={text} y={-i * strip} width={width} size={size} color={color} />
        {(pose.cover ?? 0) > 0 && <Rect width={width * pose.cover!} height={strip} fill={color} />}
      </Group>;
    })}
  </Group>;
}

export function Bars({ x = LEFT, y = WORD_Y, width = WORD_WIDTH,
  bands, color = PAPER }: {
  x?: number; y?: number; width?: number; color?: string;
  bands: (index: number) => BandMotion;
}) {
  const poses = Array.from({ length: BANDS }, (_, i) => bands(i));
  if (poses.every(p => !p.x && !p.y && (p.stretch === undefined || p.stretch === 1))) {
    return <Rect x={x} y={y} width={width} height={CAP_HEIGHT} fill={color} />;
  }
  return <Group x={x} y={y}>
    {poses.map((pose, i) => {
      return <Rect key={i} x={pose.x ?? 0} y={i * BAND_HEIGHT + (pose.y ?? 0)}
        width={width * (pose.stretch ?? 1)} height={BAND_HEIGHT} fill={color} />;
    })}
  </Group>;
}

export function EditorialFrame({ frame }: { frame: number }) {
  const chapter = [...CHAPTERS].reverse().find(c => frame >= c.at)!;
  return <>
    {/* Reserve title and caption areas even during oversized motion. */}
    <Rect width={1080} height={355} fill={INK} />
    <Rect y={1390} width={1080} height={530} fill={INK} />
    <Label text="DON'T BLINK" y={224} size={24} />
    <Label text="A PERCEPTION STUDY" y={259} size={18} color="#F8F5EF99" />
    <Rect x={LEFT} y={315} width={800} height={2} fill="#F8F5EF40" />
    <Rect x={858} y={228} width={18} height={18} fill={RED} />
    <Label text={chapter.label} y={1430} size={20} />
    <Label text={chapter.note} y={1482} size={18} color="#F8F5EF99" />
    <Rect x={LEFT} y={1550} width={800} height={2} fill="#F8F5EF40" />
    <Rect x={LEFT} y={1547} width={800 * frame / 539} height={6} fill={RED} />
    <Label text="18 SECONDS. ONE CHANCE." y={1585} size={18} />
    <Label text="KEEP LOOKING" x={735} y={1585} size={18} />
  </>;
}
