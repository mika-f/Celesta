import { useCurrentFrame } from '@celesta/react';
import { BUILD_AT } from '../constants';
import { CodePanel } from '../components/CodePanel';
import { DemoPanel } from '../components/Frame';
import type { TipId } from '../demos';
import { getPlan } from '../plan';

// Bars 1–7: the snippet types in line by line on the beat, and each stage
// of the demo appears as the line that makes it is finished.
export function Build({ tip, accent }: { tip: TipId; accent: string }) {
  const frame = useCurrentFrame() + BUILD_AT;
  return (
    <>
      <CodePanel frame={frame} accent={accent} />
      <DemoPanel tip={tip} frame={frame} stageAt={getPlan().stageAt} accent={accent} />
    </>
  );
}
