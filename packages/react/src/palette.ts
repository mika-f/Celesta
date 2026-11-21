// Whole-film palettes: every color comes from four roles (`ground`, `main`,
// `second`, `accent`) and named blends of them, so picking another palette
// recolors the film. A palette is a set of `<Theme>` color names
// (`<Theme palette>`), and the palettes of a film are the options of a
// `select` project property (`palettes.property()`), so the Properties
// panel, `props.json`, and `--props` pick one without editing code.

import { isHexColor, parseHexColor } from '@celesta.studio/math';

import type { JsonValue } from './generated/serde_json/JsonValue';

/** The four roles every palette gives a color. */
export const PALETTE_ROLES = ['ground', 'main', 'second', 'accent'] as const;

/** `ground` lies behind everything, `main` is the chief color, `second` is ink for words, `accent` a touch here and there. */
export type PaletteRole = (typeof PALETTE_ROLES)[number];

/** A palette's four roles, each a `#RRGGBB` or `#RRGGBBAA` color. */
export type PaletteRoles = Readonly<Record<PaletteRole, string>>;

/**
 * A color made from the palette's own: a role, an earlier derived name, or a
 * hex color. Make one with `mix()` or `alpha()`.
 */
export type DerivedColor =
  | { readonly kind: 'mix'; readonly from: string; readonly to: string; readonly amount: number }
  | { readonly kind: 'alpha'; readonly color: string; readonly amount: number };

/**
 * `amount` of the way from `from` to `to`, blended in sRGB like CSS
 * `color-mix(in srgb, from (1 - amount), to amount)`. Each side is a role, a
 * derived name defined before this one, or a hex color.
 */
export function mix(from: string, to: string, amount: number): DerivedColor {
  return { kind: 'mix', from, to, amount };
}

/**
 * `color` at `amount` of its opacity, like CSS
 * `color-mix(in srgb, color amount, transparent)`: the see-through form of a
 * role, never the color again with a hand-written alpha.
 */
export function alpha(color: string, amount: number): DerivedColor {
  return { kind: 'alpha', color, amount };
}

/** One palette: its roles and every color name it gives a `<Theme>`. */
export interface Palette {
  readonly name: string;
  readonly roles: PaletteRoles;
  /** The roles and the derived colors by name, each a `#RRGGBB` or `#RRGGBBAA` color. */
  readonly colors: Readonly<Record<string, string>>;
}

/** Something to fix in a palette that does not stop it from drawing. */
export interface PaletteIssue {
  readonly palette: string;
  readonly code: 'low-contrast';
  /** `second` on `ground`, as `n:1`. */
  readonly contrast: number;
  readonly message: string;
}

export interface DefinePalettesOptions {
  /** Named colors made from the roles; each may use the names before it. */
  derived?: Readonly<Record<string, DerivedColor>>;
  /** The palette `get()` and `property()` default to; the first one when omitted. */
  default?: string;
}

/** The `select` project property field that picks a palette. */
export interface PalettePropertyField {
  type: 'select';
  label?: string;
  defaultValue: string;
  options: readonly string[];
}

/** A film's palettes, from `definePalettes()`. */
export interface PaletteSet {
  /** The palette names, in the order they were defined. */
  readonly names: readonly string[];
  /** The name `get()` returns without one. */
  readonly defaultName: string;
  /** Every palette by name. */
  readonly palettes: Readonly<Record<string, Palette>>;
  /**
   * The palette named `name`, or the default one when `name` is `undefined`.
   * Takes a project property's value as it is:
   * `palettes.get(getProjectProperty('palette'))`. Throws on any other name.
   */
  get(name?: JsonValue): Palette;
  /** A `select` field for `defineProjectProperties()` whose options are the palette names. */
  property(options?: { label?: string; defaultValue?: string }): PalettePropertyField;
  /** The palettes whose `second` is below 4.5:1 contrast on their `ground`. */
  check(): PaletteIssue[];
}

/** WCAG's minimum contrast for text, which `second` must reach on `ground`. */
export const MIN_TEXT_CONTRAST = 4.5;

type Rgba = [number, number, number, number];

function hex([r, g, b, a]: Rgba): string {
  const byte = (value: number) => Math.round(Math.min(255, Math.max(0, value))).toString(16).padStart(2, '0');
  return `#${byte(r)}${byte(g)}${byte(b)}${a >= 255 ? '' : byte(a)}`.toUpperCase();
}

// CSS color-mix in srgb: interpolate premultiplied, then unpremultiply.
function mixRgba(from: Rgba, to: Rgba, amount: number): Rgba {
  const a = from[3] * (1 - amount) + to[3] * amount;
  if (a === 0) return [0, 0, 0, 0];
  const channel = (i: number) => (from[i]! * from[3] * (1 - amount) + to[i]! * to[3] * amount) / a;
  return [channel(0), channel(1), channel(2), a];
}

function luminance([r, g, b]: Rgba): number {
  const linear = (value: number) => {
    const c = value / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/**
 * The WCAG contrast ratio of `foreground` drawn over `background`, both hex
 * colors: a see-through foreground is first laid over the background, whose
 * own alpha is ignored. From 1 to 21.
 */
export function contrastRatio(foreground: string, background: string): number {
  const back = parseHexColor(background);
  const front = parseHexColor(foreground);
  if (!back || !front) throw new Error(`contrastRatio() takes hex colors, got ${JSON.stringify(foreground)} and ${JSON.stringify(background)}`);
  const opaqueBack: Rgba = [back[0], back[1], back[2], 255];
  const over = front[3] / 255;
  const lay = (i: 0 | 1 | 2) => front[i] * over + opaqueBack[i] * (1 - over);
  const shown: Rgba = [lay(0), lay(1), lay(2), 255];
  const [light, dark] = [luminance(shown), luminance(opaqueBack)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

function checkAmount(name: string, amount: unknown): number {
  if (typeof amount !== 'number' || !Number.isFinite(amount) || amount < 0 || amount > 1) {
    throw new Error(`palette color "${name}" amount must be a number from 0 to 1, got ${JSON.stringify(amount)}`);
  }
  return amount;
}

function resolvePalette(name: string, roles: PaletteRoles, derived: Readonly<Record<string, DerivedColor>>): Palette {
  if (roles === null || typeof roles !== 'object') throw new Error(`palette "${name}" must be an object of ${PALETTE_ROLES.join(', ')}`);
  const problems: string[] = [];
  for (const role of PALETTE_ROLES) {
    if (!isHexColor(roles[role])) {
      problems.push(`${role} must be a #RRGGBB or #RRGGBBAA color, got ${JSON.stringify(roles[role])}`);
    }
  }
  for (const key of Object.keys(roles)) {
    if (!(PALETTE_ROLES as readonly string[]).includes(key)) {
      problems.push(`"${key}" is not a role (${PALETTE_ROLES.join(', ')}); name colors made from the roles in derived`);
    }
  }
  if (problems.length > 0) throw new Error(`palette "${name}": ${problems.join('; ')}`);

  const colors: Record<string, string> = {};
  for (const role of PALETTE_ROLES) colors[role] = roles[role];
  const operand = (color: string, what: string): Rgba => {
    const value = Object.prototype.hasOwnProperty.call(colors, color) ? colors[color]! : color;
    const rgba = isHexColor(value) ? parseHexColor(value) : undefined;
    if (!rgba) {
      throw new Error(
        `palette color "${what}" uses ${JSON.stringify(color)}, which is neither a hex color nor a role or derived color defined before it (${Object.keys(colors).join(', ')})`,
      );
    }
    return rgba;
  };
  for (const [key, color] of Object.entries(derived)) {
    if (color?.kind === 'mix') {
      const amount = checkAmount(key, color.amount);
      colors[key] = hex(mixRgba(operand(color.from, key), operand(color.to, key), amount));
    } else if (color?.kind === 'alpha') {
      const amount = checkAmount(key, color.amount);
      const [r, g, b, a] = operand(color.color, key);
      colors[key] = hex([r, g, b, a * amount]);
    } else {
      throw new Error(`palette color "${key}" must be made with mix() or alpha()`);
    }
  }
  return { name, roles: { ...roles }, colors };
}

/**
 * Defines a film's palettes: each names a color for the four roles, and
 * `derived` names the blends every palette makes from its own roles. Throws
 * when a role is not a hex color or a derived color cannot be made, and
 * warns (`console.warn`) for each palette whose `second` is below 4.5:1
 * contrast on its `ground`. Call it at module level.
 */
export function definePalettes(
  palettes: Readonly<Record<string, PaletteRoles>>,
  options: DefinePalettesOptions = {},
): PaletteSet {
  const derived = options.derived ?? {};
  for (const key of Object.keys(derived)) {
    if ((PALETTE_ROLES as readonly string[]).includes(key)) {
      throw new Error(`palette color "${key}" is a role; give the derived color another name`);
    }
  }
  const names = Object.keys(palettes);
  if (names.length === 0) throw new Error('definePalettes() needs at least one palette');
  const resolved: Record<string, Palette> = {};
  for (const name of names) resolved[name] = resolvePalette(name, palettes[name]!, derived);

  const defaultName = options.default ?? names[0]!;
  if (!names.includes(defaultName)) {
    throw new Error(`definePalettes() default ${JSON.stringify(defaultName)} is not a palette (${names.join(', ')})`);
  }

  const issues = names.flatMap((name): PaletteIssue[] => {
    const { second, ground } = resolved[name]!.roles;
    const contrast = contrastRatio(second, ground);
    if (contrast >= MIN_TEXT_CONTRAST) return [];
    const ratio = Math.floor(contrast * 100) / 100;
    return [{
      palette: name,
      code: 'low-contrast',
      contrast,
      message: `palette "${name}": second ${second} on ground ${ground} has ${ratio}:1 contrast, below ${MIN_TEXT_CONTRAST}:1 for text`,
    }];
  });
  for (const issue of issues) console.warn(`Celesta: ${issue.message}`);

  return {
    names,
    defaultName,
    palettes: resolved,
    get(name) {
      if (name === undefined) return resolved[defaultName]!;
      if (typeof name === 'string' && Object.prototype.hasOwnProperty.call(resolved, name)) return resolved[name]!;
      throw new Error(`${JSON.stringify(name)} is not a palette (${names.join(', ')})`);
    },
    property({ label, defaultValue = defaultName } = {}) {
      if (!names.includes(defaultValue)) {
        throw new Error(`palette property default ${JSON.stringify(defaultValue)} is not a palette (${names.join(', ')})`);
      }
      return { type: 'select', ...(label === undefined ? {} : { label }), defaultValue, options: names };
    },
    check() {
      return [...issues];
    },
  };
}
