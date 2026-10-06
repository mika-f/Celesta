// Chapter 3: kinetic words that change size every frame, a Japanese
// paragraph wrapped by phrase and typed on, a counter with a stroke and a
// shadow, and a scrolling ticker clipped to a band.
import { Easings, Group, Rect, Text, TextBox, interpolate, progress, useCountUp, useCurrentFrame, useTextMetrics, useTypewriter } from '@celesta/react';

import { DIM, DISPLAY, FPS, INK, MONO, PALETTE, style } from '../shared';

const WORDS = ['FRAME', 'EXACT', 'REACT', 'SCENE', 'LAYER', 'BLEND', 'GLYPH', 'SHAPE', 'TRACK', 'RENDER', 'EXPORT', 'PIXEL'];

const PARAGRAPH =
  'Celesta はコードで動画を書くためのツールです。タイムラインを JSON や React で記述し、' +
  'プレビューしながらフレーム単位で正確に MP4 へ書き出せます。テキストは文節で折り返され、' +
  '日本語の見出しも自然な位置で改行されます。';

const TICKER = 'GPU EXPORT · PIPELINED READBACK · YUV420P ON THE GPU · TILED PATH COVERAGE · PAIRED GAUSSIAN TAPS · ';

function Words({ t }: { t: number }) {
  return (
    <Group x={80} y={100}>
      {WORDS.map((word, i) => {
        const pulse = 0.5 + 0.5 * Math.sin(t * 4 - i * 0.6);
        return (
          <Text key={word} y={i * 72} scale={0.8 + 0.4 * pulse} opacity={0.35 + 0.65 * pulse}
            style={style(DISPLAY, 64, i % 3 === 0 ? PALETTE[i % 6] : INK, { letterSpacing: 4 + 12 * pulse })}>
            {word}
          </Text>
        );
      })}
    </Group>
  );
}

function Paragraph({ frame }: { frame: number }) {
  const { text, caretVisible } = useTypewriter(PARAGRAPH, { from: 6, framesPerChar: 0.9 });
  return (
    <Group x={760} y={110}>
      <Rect x={-30} y={-30} width={1060} height={420} cornerRadius={24} fill="#0E1230D0" stroke="#3DA5FF60" strokeWidth={2}
        shadow={{ color: '#00000090', blur: 30, offsetX: 0, offsetY: 18 }} />
      <TextBox width={1000} height={360} minFontSize={30} maxFontSize={40} lineHeight={1.5} overflow="visible"
        style={{ fill: { type: 'solid', color: INK }, lineBreak: 'phrase', lang: 'ja' }}>
        {text + (caretVisible ? '▍' : '')}
      </TextBox>
      <Text x={0} y={400} style={style(MONO, 18, DIM)}>
        {`${[...text].length} / ${[...PARAGRAPH].length} chars · phrase line breaking · frame ${frame}`}
      </Text>
    </Group>
  );
}

function Counter({ frame }: { frame: number }) {
  const value = useCountUp(1_660_000, { delay: 10, durationInFrames: 100 });
  const sweep = progress(frame, 10, 100, Easings.easeOutExpo);
  return (
    <Group x={760} y={600}>
      <Text style={style(MONO, 22, DIM, { letterSpacing: 5 })}>LAYERS RENDERED</Text>
      <Text y={40} shadow={{ color: '#FF3D7F80', blur: 16, offsetX: 6, offsetY: 6 }}
        style={style(DISPLAY, 180, '#FFFFFF', {
          letterSpacing: 6,
          stroke: { paint: { type: 'solid', color: '#FF3D7F' }, width: 4 },
        })}>
        {value.toLocaleString('en-US')}
      </Text>
      <Rect y={250} width={1000} height={10} cornerRadius={5} fill="#FFFFFF20" />
      <Rect y={250} width={1000 * sweep} height={10} cornerRadius={5} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 1000, y: 0 },
        stops: [{ offset: 0, color: '#00E0B8' }, { offset: 1, color: '#3DA5FF' }],
      }} />
    </Group>
  );
}

const TICKER_STYLE = style(DISPLAY, 52, '#0B0E24', { letterSpacing: 6 });

function Ticker({ t }: { t: number }) {
  const { width } = useTextMetrics(TICKER, TICKER_STYLE);
  const offset = (t * 420) % width;
  return (
    <Group y={960} clip={{ x: 0, y: 0, width: 1920, height: 70 }}>
      <Rect width={1920} height={70} fill="#7B5CFF" />
      {[0, 1, 2].map((k) => (
        <Text key={k} x={-offset + k * width} y={50} anchorY="baseline" style={TICKER_STYLE}>
          {TICKER}
        </Text>
      ))}
    </Group>
  );
}

export function Typography() {
  const frame = useCurrentFrame();
  const t = frame / FPS;
  return (
    <Group opacity={interpolate(frame, [0, 12], [0.4, 1], { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' })}>
      <Words t={t} />
      <Paragraph frame={frame} />
      <Counter frame={frame} />
      <Ticker t={t} />
    </Group>
  );
}
