import { Easings, Group, Rect, Video, useCurrentFrame } from '@celesta/react';
import { ToolTag } from '../components/Chrome';
import { Label } from '../components/Label';
import { H, ORDER, TOOL, W } from '../constants';
import { progress } from '../helpers';

const SLICE = W / 3;

// One picture stitched from three exports: each third comes from a different
// renderer, at the same moment of the scene.
export function Same() {
  const f = useCurrentFrame();
  const title = progress(f, 30, 24, Easings.easeOutExpo);
  return (
    <>
      {ORDER.map((id, i) => {
        const open = progress(f, i * 6, 22, Easings.easeOutExpo);
        return (
          <Group key={id} clip={{ x: i * SLICE, y: 0, width: SLICE, height: H * open }}>
            <Video src={`./bench/out/${id}.mp4`} startFrom={3} />
          </Group>
        );
      })}
      {[1, 2].map((i) => (
        <Rect key={i} x={i * SLICE - 1} y={0} width={2} height={H * progress(f, 10 + i * 6, 26, Easings.easeOutExpo)} fill="#FFFFFFCC" />
      ))}
      <Rect width={W} height={340} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 340 },
        stops: [{ offset: 0, color: '#05060CFA' }, { offset: 0.55, color: '#05060CD0' }, { offset: 1, color: '#05060C00' }],
      }} opacity={title} />
      <Rect y={H - 280} width={W} height={280} fill={{
        type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 280 },
        stops: [{ offset: 0, color: '#05060C00' }, { offset: 0.5, color: '#05060CD8' }, { offset: 1, color: '#05060CFA' }],
      }} />
      <Group x={96} y={90} opacity={title}>
        <Rect y={6} width={36} height={4} fill={TOOL.celesta.color} />
        <Label x={52} size={18} font="mono" weight={700} color={TOOL.celesta.color} spacing={3}>02 — SAME PICTURE</Label>
        <Group y={40} clip={{ x: -8, y: -10, width: 1800, height: 92 }}>
          <Label y={80 * (1 - title)} size={72} weight={700} spacing={-2}>One frame, three renderers, stitched.</Label>
        </Group>
      </Group>
      {ORDER.map((id, i) => {
        const p = progress(f, 50 + i * 8, 20, Easings.easeOutExpo);
        return (
          <Group key={id} x={i * SLICE + 48} y={H - 130 + 20 * (1 - p)} opacity={p}>
            <ToolTag id={id} size={34} />
          </Group>
        );
      })}
    </>
  );
}
