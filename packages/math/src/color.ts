// Hex colors as every Celesta prop and the renderer take them, mirroring
// `celesta_composition::Color::from_hex`.

const HEX_COLOR = /^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})?$/;

/** Whether `value` is a `#RRGGBB` or `#RRGGBBAA` color, digits in either case. */
export function isHexColor(value: unknown): value is string {
  return typeof value === 'string' && HEX_COLOR.test(value);
}

/**
 * A `#RRGGBB` or `#RRGGBBAA` color as `[r, g, b, a]`, each 0–255 (`#RRGGBB`
 * is opaque), or `undefined` when `color` is neither.
 */
export function parseHexColor(color: string): [number, number, number, number] | undefined {
  const match = HEX_COLOR.exec(color);
  if (!match) return undefined;
  const channel = (hex: string | undefined) => (hex === undefined ? 255 : parseInt(hex, 16));
  return [channel(match[1]), channel(match[2]), channel(match[3]), channel(match[4])];
}
