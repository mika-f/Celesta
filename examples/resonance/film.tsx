import { Assets, Audio, Composition, Font, Group, Rect, Text, interpolateColor } from '@celesta.studio/react';
import { loadAudioSpectrum, useAudioSpectrum } from '@celesta.studio/media-utils';
import type { AudioSpectrum } from '@celesta.studio/media-utils';

const WIDTH = 1920;
const HEIGHT = 1080;
const FPS = 30;
const SECONDS = 16;
const SCORE = './assets/score.wav';

const INK = '#0B0D12';
const PAPER = '#E9E4DA';
const DIM = '#5C6170';
const SPECTRUM = ['#2F6BFF', '#19C3B4', '#F2C14E', '#F55138'];

// Three analyses of the one score, each read per frame by useAudioSpectrum().
let ring: AudioSpectrum;
let floor: AudioSpectrum;
let glow: AudioSpectrum;

export async function prepare() {
  // Fast attack, short release: bars that snap to each hit.
  ring = await loadAudioSpectrum(SCORE, {
    fftSize: 4096, bands: 64, scale: 'log', minFrequency: 30, maxFrequency: 12000,
    minDecibels: -80, maxDecibels: -8, smoothing: { attack: 0.005, release: 0.12 },
  });
  floor = await loadAudioSpectrum(SCORE, {
    fftSize: 2048, bands: 48, scale: 'mel', maxFrequency: 12000,
    minDecibels: -82, maxDecibels: -18, smoothing: { attack: 0.01, release: 0.25 },
  });
  // A slow release for the loudness glow.
  glow = await loadAudioSpectrum(SCORE, { bands: 1, smoothing: { attack: 0.02, release: 0.6 } });
}

function Label({ children, x, y, size = 20, color = PAPER, anchorX = 0 }: {
  children: string; x: number; y: number; size?: number; color?: string; anchorX?: number;
}) {
  return <Text x={x} y={y} anchorX={anchorX} style={{
    fontFamily: 'IBM Plex Mono', fontSize: size, fill: { type: 'solid', color },
  }}>{children}</Text>;
}

function bandColor(band: number, bands: number): string {
  return interpolateColor(band / (bands - 1), [0, 0.35, 0.7, 1], SPECTRUM);
}

const CENTER = { x: WIDTH / 2, y: 470 };
const RADIUS = 170;
const REACH = 160;

/** Every band twice, mirrored left and right, low frequencies at the top. */
function Ring() {
  const { bands } = useAudioSpectrum(ring);
  const { rms } = useAudioSpectrum(glow);
  const count = bands.length;
  return <Group x={CENTER.x} y={CENTER.y}>
    <Rect width={RADIUS * 2 + rms * 420} height={RADIUS * 2 + rms * 420} cornerRadius={RADIUS + rms * 210}
      anchorX={0.5} anchorY={0.5} fill={SPECTRUM[0]} opacity={0.06 + rms * 0.4} />
    {bands.flatMap((level, band) => [-1, 1].map((side) => {
      const angle = -90 + side * (band + 0.5) * (180 / count);
      return <Group key={`${band}:${side}`} rotation={angle}>
        <Rect x={RADIUS + 12} width={5 + level * REACH} height={7} cornerRadius={3.5}
          anchorY={0.5} fill={bandColor(band, count)} opacity={0.35 + level * 0.65} />
      </Group>;
    }))}
    <Rect width={RADIUS * 2} height={RADIUS * 2} cornerRadius={RADIUS}
      anchorX={0.5} anchorY={0.5} fill={INK} stroke={DIM} strokeWidth={1.5} />
  </Group>;
}

function Meter() {
  const { rms } = useAudioSpectrum(glow);
  const decibels = rms > 0 ? 20 * Math.log10(rms) : -Infinity;
  const reading = Number.isFinite(decibels) ? decibels.toFixed(1).replace('-', '−') : '−∞';
  return <Group x={CENTER.x} y={CENTER.y}>
    <Label x={0} y={-70} size={18} color={DIM} anchorX={0.5}>RMS</Label>
    <Text x={0} y={-44} anchorX={0.5} style={{
      fontFamily: 'Bebas Neue', fontSize: 88, fill: { type: 'solid', color: PAPER },
    }}>{reading}</Text>
    <Label x={0} y={48} size={18} color={DIM} anchorX={0.5}>dBFS</Label>
    <Rect x={-100} y={84} width={200} height={4} fill={DIM} />
    <Rect x={-100} y={84} width={Math.min(1, rms * 2.5) * 200} height={4} fill={SPECTRUM[3]} />
  </Group>;
}

const FLOOR = { x: 96, y: 1000, width: WIDTH - 192, height: 150 };

function Floor() {
  const { bands } = useAudioSpectrum(floor);
  const step = FLOOR.width / bands.length;
  return <Group x={FLOOR.x} y={FLOOR.y}>
    {bands.map((level, band) => (
      <Rect key={band} x={band * step + 3} width={step - 6} height={4 + level * FLOOR.height}
        anchorY={1} cornerRadius={2} fill={bandColor(band, bands.length)} opacity={0.3 + level * 0.7} />
    ))}
    <Rect y={2} width={FLOOR.width} height={1.5} fill={DIM} />
  </Group>;
}

export default function Resonance() {
  return <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={SECONDS * FPS}>
    <Assets>
      <Font src="../afterimage/assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="../afterimage/assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Rect width={WIDTH} height={HEIGHT} fill={INK} />
    <Text x={96} y={64} style={{
      fontFamily: 'Bebas Neue', fontSize: 120, fill: { type: 'solid', color: PAPER },
    }}>RESONANCE</Text>
    <Label x={100} y={190} color={DIM}>CELESTA / AUDIO SPECTRUM</Label>
    <Label x={WIDTH - 96} y={92} anchorX={1}>useAudioSpectrum()</Label>
    <Label x={WIDTH - 96} y={124} anchorX={1} color={DIM}>ANALYZED ONCE IN prepare()</Label>
    <Label x={WIDTH - 96} y={156} anchorX={1} color={DIM}>READ PER FRAME</Label>
    <Ring />
    <Meter />
    <Label x={96} y={800} color={DIM}>RING  LOG / 64 BANDS / FFT 4096 / 30 Hz–12 kHz</Label>
    <Label x={WIDTH - 96} y={800} anchorX={1} color={DIM}>FLOOR  MEL / 48 BANDS / FFT 2048</Label>
    <Floor />
    <Label x={96} y={1024} size={16} color={DIM}>LOW</Label>
    <Label x={WIDTH - 96} y={1024} size={16} color={DIM} anchorX={1}>HIGH</Label>
    <Audio src={SCORE} />
  </Composition>;
}
