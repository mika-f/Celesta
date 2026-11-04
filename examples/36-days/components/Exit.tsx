import type { ReactNode } from 'react';
import { Easings } from '@celesta.studio/react';
import { Transition } from '@celesta.studio/transitions';

export function Exit({ children }: { children: ReactNode }) {
  return (
    <Transition type={['fade', 'slide']} direction="out" slideFrom="top" distance={30}
      durationInFrames={10} easing={Easings.easeInCubic}>
      {children}
    </Transition>
  );
}
