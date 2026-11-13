import { Group, Rect, useTextMetrics } from '@celesta/react';
import { BAND_HEIGHT, CAP_HEIGHT, INK, LEFT, PAPER, RED, TYPE_SIZE, WORD_WIDTH,
  WORD_Y, direction, hit, mix, move } from './motion';
import { Bars, Label, SlicedWord, Word } from './type';

export function Challenge({ frame: f }: { frame: number }) {
  const exit = move(f, 44, 59);
  const disturbance = f >= 31 && f < 38 ? Math.sin((f - 31) / 7 * Math.PI) * 22 : 0;
  return <>
    <SlicedWord text="DON'T" y={mix(482, -40, exit)} bands={i => ({ x: direction(i) * disturbance })} />
    <SlicedWord text="BLINK" y={mix(920, WORD_Y, exit)} bands={i => ({
      x: direction(i) * (1 - hit(f, 3 + i * 2, 16 + i * 2)) * 1100,
    })} />
    <Rect x={LEFT} y={mix(873, 685, exit)} width={800} height={6} fill={RED} />
  </>;
}

export function Deconstruction({ frame: f }: { frame: number }) {
  const split = move(f, 68, 92);
  const join = move(f, 140, 155);
  const pose = (i: number) => ({
    x: direction(i) * 86 * split * (1 - join),
    y: (i - 2) * 62 * split * (1 - join),
    stretch: 1 - 0.16 * split * (1 - join),
  });
  return <>
    {f < 128 ? <SlicedWord text="BLINK" bands={i => ({ ...pose(i),
      cover: move(f, 104 + i * 2, 118 + i * 2),
    })} /> : f < 156 ? <Bars bands={pose} /> : <>
      <SlicedWord text="LOOK" />
      {Array.from({ length: 5 }, (_, i) => <Rect key={i}
        x={LEFT + WORD_WIDTH * hit(f, 156 + i * 2, 167 + i * 2)}
        y={WORD_Y + i * BAND_HEIGHT} width={WORD_WIDTH * (1 - hit(f, 156 + i * 2, 167 + i * 2))}
        height={BAND_HEIGHT + 0.7} fill={PAPER} />)}
    </>}
    <Rect x={LEFT} y={685} width={800 * (1 - move(f, 60, 74))} height={6} fill={RED} />
    {f >= 91 && f < 153 && <Label text="B / L / I / N / K" y={1255} size={23} color={RED} />}
  </>;
}

/** OO counters become eyes, then the same horizontal strips become AGAIN. */
export function Look({ frame: f }: { frame: number }) {
  const style = { fontFamily: 'Bebas Neue', fontSize: TYPE_SIZE };
  const metrics = useTextMetrics('LOOK', style);
  const sx = WORD_WIDTH / metrics.width;
  const glance = move(f, 211, 219) - move(f, 242, 249);
  const lid = move(f, 231, 234) - move(f, 234, 240);
  const out = move(f, 265, 279);
  return <>
    {f < 280 ? <SlicedWord text="LOOK" bands={i => ({
      x: direction(i) * 12 * (move(f, 194, 198) - move(f, 198, 204)), cover: out,
    })} /> : <>
      <SlicedWord text="AGAIN" />
      {Array.from({ length: 5 }, (_, i) => {
        const reveal = hit(f, 280 + i * 2, 291 + i * 2);
        return <Rect key={i} x={LEFT + WORD_WIDTH * reveal}
          y={WORD_Y + i * BAND_HEIGHT} width={WORD_WIDTH * (1 - reveal)}
          height={BAND_HEIGHT + 0.7} fill={PAPER} />;
      })}
    </>}
    {f >= 210 && f < 262 && metrics.glyphs.slice(1, 3).map((glyph, i) => {
      const cx = LEFT + (glyph.x + glyph.width / 2) * sx;
      return <Group key={i}>
        <Rect x={cx + 21 * glance} y={WORD_Y + CAP_HEIGHT * 0.5}
          width={28} height={28} cornerRadius={14} fill={RED}
          scale={hit(f, 210 + i * 3, 217 + i * 3) * (1 - move(f, 254, 261))}
          anchorX={0.5} anchorY={0.5} />
        <Rect x={cx - 46} y={WORD_Y + CAP_HEIGHT * 0.2}
          width={92} height={CAP_HEIGHT * 0.31 * lid} fill={PAPER} />
        <Rect x={cx - 46} y={WORD_Y + CAP_HEIGHT * 0.8 - CAP_HEIGHT * 0.31 * lid}
          width={92} height={CAP_HEIGHT * 0.31 * lid} fill={PAPER} />
      </Group>;
    })}
    {f >= 215 && f < 260 && <Label text="YES. YOU." y={1255} size={23} color={RED} />}
  </>;
}

export function Again({ frame: f }: { frame: number }) {
  const burst = move(f, 318, 342);
  const close = move(f, 398, 414);
  const spread = move(f, 345, 359);
  const rejoin = move(f, 377, 389);
  return <>
    <Group x={484} y={WORD_Y + CAP_HEIGHT / 2}
      scale={mix(1, 2.35, burst) * mix(1, 0.28, close)}>
      <SlicedWord text="AGAIN" x={-400} y={-CAP_HEIGHT / 2} bands={i => ({
        x: direction(i) * 70 * (spread - rejoin), y: (i - 2) * 135 * (spread - rejoin),
      })} />
    </Group>
    {/* A counter-sized window reveals a tiny copy inside the oversized word. */}
    {f >= 355 && f < 391 && <Group x={484} y={WORD_Y + CAP_HEIGHT / 2}>
      <Rect width={280} height={126} x={-140} y={-63} fill={RED} />
      <Word text="AGAIN" x={-112} y={-38} width={224} size={108} color={INK} />
    </Group>}
    {/* Close like an eyelid, then open on YOU at frame 420. */}
    {f >= 398 && <Rect x={LEFT} y={mix(WORD_Y + 112, 914, close)}
      width={800} height={mix(147, 8, close)} fill={RED} />}
    {f >= 398 && <>
      <Rect y={355} width={1080} height={559 * close} fill={INK} />
      <Rect y={mix(1390, 922, close)} width={1080} height={468 * close} fill={INK} />
      <Rect x={LEFT} y={914} width={800} height={8 * close} fill={RED} />
    </>}
  </>;
}

export function Suspense({ frame: f }: { frame: number }) {
  const open = hit(f, 420, 431);
  return <>
    <Group x={LEFT} y={WORD_Y} clip={{ y: 176 * (1 - open), width: 800, height: 8 + (CAP_HEIGHT - 8) * open }}>
      <Word text="YOU" width={800} />
    </Group>
    <Rect x={LEFT} y={914 - 176 * open} width={800 * (1 - open)} height={8} fill={RED} />
    <Rect x={LEFT} y={1200} width={800} height={2} fill="#F8F5EF40" />
    <Rect x={LEFT + 800 * move(f, 450, 505)} y={1196} width={8} height={10} fill={RED} />
  </>;
}

export function Punchline({ frame: f }: { frame: number }) {
  const reframe = hit(f, 510, 517);
  const stamp = hit(f, 510, 519);
  return <>
    <Word text="YOU" x={LEFT} y={mix(WORD_Y, 552, reframe)} width={800} />
    <Group x={LEFT} y={1050} clip={{ width: 800, height: 204 }}>
      <Rect width={800} height={204} fill={RED} x={-800 * (1 - stamp)} />
      <Word text="BLINKED." x={28} y={mix(226, 30, stamp)} width={744} size={205} color={INK} />
    </Group>
    <Rect x={LEFT} y={1298} width={800} height={6} fill={PAPER} />
  </>;
}
