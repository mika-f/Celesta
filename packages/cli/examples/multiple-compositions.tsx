import { Composition, Rect, Text, defineProjectProperties, getProjectProperty, useVideoConfig } from '@celesta.studio/react';

// Both formats share the entry's schema, props.json and prepare() data.
defineProjectProperties({ title: { type: 'string', defaultValue: 'One entry, two formats' } });

function Title() {
  const { width, height } = useVideoConfig();
  return <>
    <Rect width={width} height={height} fill="#171728" />
    <Text x={width / 2} y={height / 2} anchorX={0.5} anchorY={0.5}
      style={{ fontSize: 48, fill: { type: 'solid', color: '#f4f4ff' } }}>{getProjectProperty('title')}</Text>
  </>;
}

export default function Root() {
  return <>
    <Composition id="landscape" width={1280} height={720} fps={30} durationInFrames={90}><Title /></Composition>
    <Composition id="portrait" width={720} height={1280} fps={24} durationInFrames={48}><Title /></Composition>
  </>;
}
