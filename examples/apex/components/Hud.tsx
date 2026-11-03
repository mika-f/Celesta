import { FrameChrome } from '@celesta.studio/components';
import { BONE } from '../constants';

// Global frame HUD: corner brackets, timecode, and a film-long progress line.
export function Hud() {
  return (
    <FrameChrome margin={40} corners cornerLength={36} color={BONE} blendMode="difference" opacity={0.9}
      fontFamily="IBM Plex Mono" fontSize={20} timecode positions={{ timecode: 'bottom-right' }}
      progress progressHeight={3} progressInset={6.5} trackOpacity={0} />
  );
}
