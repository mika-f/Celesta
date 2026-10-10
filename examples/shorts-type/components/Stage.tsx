import type { ReactNode } from 'react';
import { Easings, Rect } from '@celesta/react';
import { Camera, SafeArea } from '@celesta/layout';
import { Transition } from '@celesta/transitions';
import { H, SAFE, W } from '../constants';

export type StageProps = {
  children: ReactNode;
  bg: string;
  zoom?: number;
  rotation?: number;
  x?: number;
  y?: number;
  shake?: number;
};

// One scene's frame: a full-bleed background, then the copy laid out inside
// the safe area and filmed by a camera centred on it, so a push-in grows
// evenly towards every edge of the safe area instead of towards the UI.
// The copy lifts out over the scene's last frames.
export function Stage({ children, bg, zoom, rotation, x, y, shake }: StageProps) {
  return (
    <>
      <Rect width={W} height={H} fill={bg} />
      <SafeArea padding={SAFE}>
        <Camera zoom={zoom} rotation={rotation} x={x} y={y} shake={shake} seed={7}>
          <Transition type={['fade', 'slide']} direction="out" slideFrom="top" distance={36}
            durationInFrames={8} easing={Easings.easeInCubic}>
            {children}
          </Transition>
        </Camera>
      </SafeArea>
    </>
  );
}
