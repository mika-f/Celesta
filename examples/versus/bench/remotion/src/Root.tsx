import { Composition } from 'remotion';
import { Nebula } from './Nebula';

export const RemotionRoot = () => (
  <Composition id="Nebula" component={Nebula} width={1920} height={1080} fps={60} durationInFrames={600} />
);
