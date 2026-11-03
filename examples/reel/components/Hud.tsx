import { FrameChrome } from '@celesta.studio/components';
import { DURATION, FPS, H, W } from '../constants';
import { SCENES } from '../timeline';

// Drawn in paper and blended with `difference`, so it reads light over the
// dark scenes and dark over the paper ones without tracking which is which.
// The accent progress bar is left out of the blend to keep its color.
export function Hud() {
  return (
    <FrameChrome margin={56} corners cornerLength={26} color="paper" blendMode="difference" opacity={0.75}
      fontFamily="mono" title="Celesta" subtitle="/ Reel 01" timecode chapters scenes={SCENES}
      caption={`${W}×${H} · ${FPS} FPS · 120 BPM`}
      progress segments={false} progressColor="accent" progressBlendMode="normal" progressInset={25}
      trackOpacity={0} durationInFrames={DURATION} />
  );
}
