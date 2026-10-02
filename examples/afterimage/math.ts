export const clamp = (n: number) => Math.max(0, Math.min(1, n));
export const ease = (n: number) => 1 - Math.pow(1 - clamp(n), 4);
export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
