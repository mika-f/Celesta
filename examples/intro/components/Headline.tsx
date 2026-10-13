import { C } from '../constants';
import { Label } from './Label';

export type HeadlineProps = {
  children: string;
  x: number;
  y: number;
  size?: number;
  color?: string;
  anchorX?: number;
  opacity?: number;
};

// A heavy Space Grotesk line: every piece of on-screen copy.
export function Headline({ children, x, y, size = 120, color = C.fg, anchorX = 0, opacity = 1 }: HeadlineProps) {
  return (
    <Label x={x} y={y} size={size} font="display" weight={700} color={color} ax={anchorX} opacity={opacity}>
      {children}
    </Label>
  );
}
