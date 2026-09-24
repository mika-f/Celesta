import {
  Composition,
  ProjectProvider,
  Text,
  defineProjectProperties,
  loadProjectFromString,
  useProjectProperty,
} from '@mikan/react';
import type { ProjectPropertySchema } from '@mikan/react';

// Declared at module scope so it has run before the CLI's startup Ready
// message is sent — the GPUI editor reads this schema off the handshake and
// renders one Inspector row per field, writing edits into the project's
// `properties` map. Values are read back here with `useProjectProperty`.
const propertySchema: ProjectPropertySchema = {
  title: { type: 'string', label: 'Title', defaultValue: 'Celesta' },
  accent: { type: 'color', label: 'Accent', defaultValue: '#ff8800' },
  fontSize: { type: 'number', label: 'Font Size', defaultValue: 48, min: 8, max: 200, step: 2 },
  showSubtitle: { type: 'boolean', label: 'Show Subtitle', defaultValue: false },
  weight: { type: 'select', label: 'Weight', defaultValue: 'bold', options: ['bold', 'light'] },
};

defineProjectProperties(propertySchema);

// A real entry would `loadProject('./project.mikan.json')`; the string form
// keeps this example self-contained.
const project = loadProjectFromString(
  JSON.stringify({
    version: 0,
    settings: {
      width: 640,
      height: 360,
      frameRate: { numerator: 30, denominator: 1 },
      sampleRate: 48000,
    },
    assets: {},
    characters: {},
    tracks: [],
    properties: { title: 'Chapter 3' },
  }),
);

function Title() {
  const title = useProjectProperty('title', 'Celesta');
  const fontSize = useProjectProperty('fontSize', 48);
  const accent = useProjectProperty('accent', '#ff8800');
  return (
    <Text x={320} y={180} anchorX={0.5} anchorY={0.5} style={{ fontSize, fill: { type: 'solid', color: accent } }}>
      {title}
    </Text>
  );
}

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <ProjectProvider project={project}>
        <Title />
      </ProjectProvider>
    </Composition>
  );
}
