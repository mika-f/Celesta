import { useCurrentFrame } from '@celesta/react';
import { C, H, W } from '../constants';
import { Label } from './Label';

// The composition frame in the bottom-right corner, for the whole video but
// the end-screen hold: every picture is computed from this number.
export function FrameCounter() {
  const frame = useCurrentFrame();
  return (
    <Label x={W - 56} y={H - 56} size={20} color={C.muted} ax={1} ay={1}>
      {`frame ${String(frame).padStart(4, '0')}`}
    </Label>
  );
}
