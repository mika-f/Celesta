import { Cubes } from './Cubes';
import { Field } from './Field';
import { Finale } from './Finale';
import { Gyro } from './Gyro';
import { Open } from './Open';
import { Spectrum } from './Spectrum';
import { Tunnel } from './Tunnel';
import { Words } from './Words';

// The film's chapters in playback order. Every cut lands on a beat (15 frames at 120 BPM).
export const SCENES = [
  { name: 'open', durationInFrames: 120, Scene: Open },
  { name: 'tunnel', durationInFrames: 240, Scene: Tunnel },
  { name: 'words', durationInFrames: 240, Scene: Words },
  { name: 'gyro', durationInFrames: 240, Scene: Gyro },
  { name: 'spectrum', durationInFrames: 240, Scene: Spectrum },
  { name: 'cubes', durationInFrames: 240, Scene: Cubes },
  { name: 'field', durationInFrames: 300, Scene: Field },
  { name: 'finale', durationInFrames: 180, Scene: Finale },
];
