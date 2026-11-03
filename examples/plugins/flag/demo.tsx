// The flag plugin's demo: the same portrait waving three ways. Build the
// plugin first, then export with its directory:
//
//   node examples/plugins/build.mjs flag
//   cargo run -p celesta-exporter -- --react examples/plugins/flag/demo.tsx out.mp4 --plugin-dir examples/plugins/dist
import { Fragment } from 'react';

import { Composition, Font, Rect, Text } from '@celesta.studio/react';
import { Flag } from '@celesta-plugin/flag';
import type { FlagProps } from '@celesta-plugin/flag';

const PORTRAIT = '../../assets/dialogue-demo/portraits/komugi-normal.png';
const LABEL = { fontFamily: 'Noto Sans JP', fontSize: 22, fill: { type: 'solid', color: '#e8eef5' } } as const;

const FLAGS: { x: number; label: string; wave: Partial<FlagProps> }[] = [
  { x: 80, label: 'default', wave: {} },
  { x: 480, label: 'amplitude 0.12, waves 2.5', wave: { amplitude: 0.12, waves: 2.5 } },
  { x: 880, label: 'speed 2, shading 0.8', wave: { speed: 2, shading: 0.8 } },
];

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={150}>
      <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700" />
      <Rect width={1280} height={720} fill="#26364a" />
      <Text x={80} y={36} style={{ ...LABEL, fontSize: 40, fontWeight: 700 }}>@celesta-plugin/flag</Text>
      <Text x={80} y={96} style={LABEL}>Meshes from a plugin process, drawn by Celesta</Text>
      {FLAGS.map(({ x, label, wave }) => (
        <Fragment key={label}>
          <Rect x={x - 8} y={190} width={8} height={470} fill="#8a6a4a" />
          {/* The layer has room above and below the image for the wave. */}
          <Flag x={x} y={160} scale={0.9} src={PORTRAIT} {...wave} />
          <Text x={x} y={670} style={LABEL}>{label}</Text>
        </Fragment>
      ))}
    </Composition>
  );
}
