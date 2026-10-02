export const clamp = (x: number) => Math.max(0, Math.min(1, x));
export const ease = (x: number) => 1 - (1 - clamp(x)) ** 4;
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
