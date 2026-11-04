// The turntable plugin's demo: the same scene turning at two speeds. Build
// the plugin first, then export with its directory:
//
//   node examples/plugins/build.mjs turntable
//   cargo run -p celesta-exporter -- --react examples/plugins/turntable/demo.tsx out.mp4 --plugin-dir examples/plugins/dist
import { Fragment } from 'react';

import { Composition, Font, Rect, Text } from '@celesta.studio/react';
import { Turntable } from '@celesta-plugin/turntable';

const LABEL = { fontFamily: 'Noto Sans JP', fontSize: 22, fill: { type: 'solid', color: '#e8eef5' } } as const;

const TURNTABLES: { x: number; label: string; speed?: number }[] = [
  { x: 48, label: 'default (0.5 turns a second)' },
  { x: 656, label: 'speed 0.375', speed: 0.375 },
];

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={150}>
      <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700" />
      <Rect width={1280} height={720} fill="#26364a" />
      <Text x={48} y={36} style={{ ...LABEL, fontSize: 40, fontWeight: 700 }}>@celesta-plugin/turntable</Text>
      <Text x={48} y={96} style={LABEL}>A 3D scene from a plugin process, drawn by Celesta</Text>
      {TURNTABLES.map(({ x, label, speed }) => (
        <Fragment key={label}>
          {/* The layer is 640x360, drawn here at 576x324. */}
          <Rect x={x} y={190} width={576} height={324} fill="#1b2736" />
          <Turntable x={x} y={190} scale={0.9} speed={speed} />
          <Text x={x} y={540} style={LABEL}>{label}</Text>
        </Fragment>
      ))}
    </Composition>
  );
}
