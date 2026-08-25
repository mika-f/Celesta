import { Composition, ProjectTimeline, registerComponent, Text } from '@mikan/react';

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

registerComponent('BossIntroduction', BossIntroduction as never);

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <ProjectTimeline />
    </Composition>
  );
}
