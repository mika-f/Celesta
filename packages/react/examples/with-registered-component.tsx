import { Composition, ProjectTimeline, registerComponent, Text } from '@celesta/react';
import type { ComponentPropertySchema } from '@celesta/react';

interface BossIntroductionProps extends Record<string, unknown> {
  bossName: string;
  level: number;
}

function BossIntroduction({ bossName, level }: BossIntroductionProps) {
  return (
    <Text style={{ fontSize: 40, fill: { type: 'solid', color: '#ff4444' } }}>
      {`${bossName} (Lv.${level})`}
    </Text>
  );
}

const bossIntroductionSchema: ComponentPropertySchema<BossIntroductionProps> = {
  bossName: { type: 'string', label: 'Boss Name', defaultValue: 'Golem' },
  level: { type: 'number', label: 'Level', defaultValue: 1, min: 1, max: 999 },
};

registerComponent('BossIntroduction', BossIntroduction as never, bossIntroductionSchema);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <ProjectTimeline />
    </Composition>
  );
}
