import { Composition, registerComponent, Text } from '@celesta/react';
import { ProjectTimeline } from '@celesta/project';
import type { ComponentPropertySchema } from '@celesta/react';

// A `type` alias, not an `interface`: registered props must be JSON
// (`Record<string, JsonValue>`), which interfaces never satisfy.
type BossIntroductionProps = {
  bossName: string;
  level: number;
};

function BossIntroduction({ bossName, level }: BossIntroductionProps) {
  return (
    <Text style={{ fontSize: 40, fill: { type: 'solid', color: '#ff4444' } }}>
      {`${bossName} (Lv.${level})`}
    </Text>
  );
}

// Both props are required by `BossIntroductionProps`, so both fields say
// `required: true` (the build fails otherwise); the editor never offers to
// remove them from a call.
const bossIntroductionSchema: ComponentPropertySchema<BossIntroductionProps> = {
  bossName: { type: 'string', label: 'Boss Name', defaultValue: 'Golem', required: true },
  level: { type: 'number', label: 'Level', defaultValue: 1, min: 1, max: 999, required: true },
};

registerComponent('BossIntroduction', BossIntroduction,bossIntroductionSchema);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <ProjectTimeline />
    </Composition>
  );
}
