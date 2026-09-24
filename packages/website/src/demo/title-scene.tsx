import {
  Composition, Easings, Group, Rect, Text,
  interpolate, useCurrentFrame,
} from '@celesta/react';

// Make it yours: change the words and colors, then save.
const TITLE = 'Make a little magic.';
const COLORS = {
  background: '#e4daf0', ink: '#57456c', accent: '#a68bbf',
};

const CREAM = '#fff9e9';
const HALO = '#ffffff1c';
const ORBIT = '#ffffff99';

// 0 → 1 over `duration` frames, starting at frame `start`.
function enter(frame: number, start: number, duration = 24) {
  return interpolate(frame, [start, start + duration], [0, 1], {
    easing: Easings.easeOutCubic,
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
}

// A four-point star: two slim diamonds crossed at the center.
function Sparkle({ x, y, size, color, scale = 1, rotation = 0 }: {
  x: number; y: number; size: number; color: string;
  scale?: number; rotation?: number;
}) {
  const side = size / Math.SQRT2;
  const diamond = (
    <Rect width={side} height={side} anchorX={0.5} anchorY={0.5}
      rotation={45} cornerRadius={side * 0.12} fill={color} />
  );
  return (
    <Group x={x} y={y} scale={scale} rotation={rotation}>
      <Group scaleX={0.34}>{diamond}</Group>
      <Group scaleY={0.34}>{diamond}</Group>
    </Group>
  );
}

// A thin ring, squashed into an ellipse and tilted.
function Orbit({ x, y, width, height, rotation }: {
  x: number; y: number; width: number; height: number; rotation: number;
}) {
  const size = Math.max(width, height);
  return (
    <Group x={x} y={y} rotation={rotation}
      scaleX={width / size} scaleY={height / size}>
      <Rect width={size} height={size} anchorX={0.5} anchorY={0.5}
        cornerRadius={size / 2} stroke={ORBIT} strokeWidth={3} />
    </Group>
  );
}

export function Scene({ title, colors }: {
  title: string; colors: typeof COLORS;
}) {
  const frame = useCurrentFrame();
  const emblem = enter(frame, 0, 30);
  const heading = enter(frame, 10);
  const twinkle = (phase: number) => 0.8 + 0.2 * Math.sin((frame + phase) / 9);
  const drift = frame * 0.05;
  // Long titles get a smaller size so they stay on one line.
  const fontSize = title.length > 22 ? 76 : 104;

  return (
    <>
      <Rect width={1920} height={1080} fill={colors.background} />
      {[1100, 820, 560].map((size) => (
        <Rect key={size} x={920} y={410} width={size} height={size}
          anchorX={0.5} anchorY={0.5} cornerRadius={size / 2} fill={HALO} />
      ))}
      <Orbit x={960} y={430} width={1760} height={560} rotation={-14 + drift} />
      <Orbit x={960} y={430} width={1300} height={380} rotation={16 - drift} />

      <Sparkle x={1560} y={270} size={72} color={colors.accent} scale={twinkle(0)} />
      <Sparkle x={340} y={820} size={104} color={colors.accent} scale={twinkle(40)} />
      <Sparkle x={1700} y={830} size={34} color={colors.accent} scale={twinkle(80)} />

      <Group x={960} y={380 + Math.sin(frame / 20) * 6} opacity={emblem}
        scale={interpolate(emblem, [0, 1], [0.6, 1])}
        rotation={interpolate(emblem, [0, 1], [-40, -9])}>
        <Sparkle x={0} y={0} size={270} color={CREAM} />
        <Sparkle x={0} y={0} size={92} color={colors.accent} rotation={45} />
      </Group>

      <Text x={960} y={610 + (1 - heading) * 24} maxWidth={1760}
        anchorX={0.5} anchorY={0.5} opacity={heading}
        style={{
          fontFamily: 'Georgia', fontSize,
          fill: { type: 'solid', color: colors.ink }, align: 'center',
        }}>
        {title}
      </Text>
      <Text x={960} y={718} anchorX={0.5} anchorY={0.5}
        opacity={enter(frame, 22) * 0.8}
        style={{
          fontSize: 24,
          fill: { type: 'solid', color: colors.ink },
        }}>
        A LITTLE IMAGINATION. ENDLESS POSSIBILITIES.
      </Text>

      <Text x={56} y={1024} anchorY={1} opacity={0.6}
        style={{ fontSize: 18, fill: { type: 'solid', color: colors.ink } }}>
        CELESTA / CREATIVE STUDIES — 001
      </Text>
      <Text x={1864} y={1024} anchorX={1} anchorY={1} opacity={0.6}
        style={{ fontSize: 18, fill: { type: 'solid', color: colors.ink } }}>
        {`FRAME ${String(frame).padStart(3, '0')}`}
      </Text>
    </>
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080}
      fps={30} durationInFrames={150}>
      <Scene title={TITLE} colors={COLORS} />
    </Composition>
  );
}
