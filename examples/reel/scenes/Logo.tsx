import type { ReactNode } from 'react';
import { clamp01, random } from '@celesta.studio/math';
import { Easings, Group, Rect, Text, progress, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import { C, H, W } from '../constants';
import { LOGO_SIZE, logo } from '../metrics';

export function Logo() {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const cx = W / 2;
  const cy = H / 2 - 30;

  const stars = Array.from({ length: 170 }, (_, i) => {
    const zoom = 1 + f * 0.0035;
    const sx = cx + (random(`star-${i}-x`) * W * 1.2 - W * 0.6) * zoom;
    const sy = cy + (random(`star-${i}-y`) * H * 1.2 - H * 0.6) * zoom;
    const size = 1 + Math.floor(random(`star-${i}-size`) * 3);
    const twinkle = 0.25 + 0.75 * (0.5 + 0.5 * Math.sin(f * 0.15 + random(`star-${i}-phase`) * 20));
    return <Rect key={i} x={sx} y={sy} width={size} height={size} fill={C.paper} opacity={twinkle * 0.5} />;
  });

  const ring = progress(f, 4, 40, Easings.easeOutCubic);
  const orbit: ReactNode[] = [];
  const tilt = (-9 * Math.PI) / 180;
  const ellipse = (a: number, rx: number, ry: number) => {
    const ex = Math.cos(a) * rx;
    const ey = Math.sin(a) * ry;
    return [cx + ex * Math.cos(tilt) - ey * Math.sin(tilt), cy + ex * Math.sin(tilt) + ey * Math.cos(tilt)];
  };
  const dots = 140;
  for (let k = 0; k < dots * ring; k++) {
    const [ox, oy] = ellipse((k / dots) * Math.PI * 2 - Math.PI / 2, 700, 200);
    orbit.push(<Rect key={k} x={ox} y={oy} anchorX={0.5} anchorY={0.5} width={3} height={3} fill={C.paper} opacity={0.3} />);
  }
  const [px, py] = ellipse(f * 0.045 - Math.PI / 2, 700, 200);

  const flash = 1 - progress(f, 0, 12, Easings.easeOutCubic);
  const hit = progress(f, 0, 24, Easings.easeOutExpo);
  const rule = progress(f, 24, 20, Easings.easeInOutExpo);
  const sub = progress(f, 36, 16, Easings.easeOutExpo);
  const fadeOut = 1 - progress(f, durationInFrames - 14, 14, Easings.easeInCubic);

  return (
    <>
      <Rect width={W} height={H} fill={C.ink} />
      <Group opacity={fadeOut}>
        {stars}
        {orbit}
        <Rect x={px} y={py} anchorX={0.5} anchorY={0.5} width={16} height={16} cornerRadius={8}
          fill={C.accent} opacity={ring} />
        {logo ? (
          // Each letter is its own Text, trimmed to its own ink, so they share a
          // baseline instead of centering: Space Grotesk's caps are 0.7 em tall,
          // so a baseline 0.35 em below `cy` centers the word like `ay={0.5}`.
          <Group x={cx} y={cy} scale={1.3 - 0.3 * hit}>
            {logo.letters.map(({ text, x }, i) => {
              const letter = progress(f, i * 2, 24, Easings.easeOutExpo);
              return (
                <Text key={i} x={x - logo!.width / 2} y={LOGO_SIZE * 0.35 + 20 * (1 - letter)} fontFamily="display" fontSize={LOGO_SIZE} fontWeight={700} color="paper"
                  anchorY="baseline" opacity={clamp01(letter * 2)}>{text}</Text>
              );
            })}
          </Group>
        ) : (
          <Text x={cx} y={cy + 20 * (1 - hit)} fontFamily="display" fontSize={LOGO_SIZE} fontWeight={700} color="paper"
            anchorX={0.5} anchorY={0.5} scale={1.3 - 0.3 * hit} opacity={clamp01(hit * 2)}>Celesta</Text>
        )}
        <Rect x={cx} y={cy + 160} anchorX={0.5} width={Math.max(1, 640 * rule)} height={2} fill={C.paper} opacity={0.5} />
        <Text x={cx} y={cy + 216 + 16 * (1 - sub)} fontFamily="mono" fontSize={26} fontWeight={400} color="grey"
          anchorX={0.5} anchorY={0.5} opacity={sub}>C O D E - F I R S T   V I D E O</Text>
      </Group>
      <Rect width={W} height={H} fill={C.paper} opacity={flash} />
    </>
  );
}
