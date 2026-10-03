export const clamp = (n: number) => Math.min(1, Math.max(0, n));
export const ease = (n: number) => 1 - (1 - clamp(n)) ** 3;
export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
