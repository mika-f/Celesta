import type { ReactNode } from 'react';
import { Easings, Transition } from '@celesta/react';

export function Exit({ children }: { children: ReactNode }) {
  return (
    <Transition type={['fade', 'slide']} direction="out" slideFrom="top" distance={30}
      durationInFrames={10} easing={Easings.easeInCubic}>
      {children}
    </Transition>
  );
}
