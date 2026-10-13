import type { ReactNode } from 'react';
import { C, H, W } from '../constants';
import { Label } from './Label';

// Stands in for a scene until it is written.
export function placeholder(name: string): () => ReactNode {
  return () => <Label x={W / 2} y={H / 2} size={48} color={C.muted} ax={0.5} ay={0.5}>{name}</Label>;
}
