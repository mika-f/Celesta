// Property tables: per primitive, the props the editor's Properties tab
// edits, grouped into its sections. Each primitive declares its table next to
// its component, typed by its props, so a renamed prop fails the build; the
// CLI sends every table in its ready message.

/** How one prop is edited, and its value when the element does not write it. */
export type PrimitiveFieldType =
  | { type: 'number'; defaultValue?: number; min?: number; max?: number; step?: number; integer?: boolean }
  | { type: 'string'; defaultValue?: string }
  | { type: 'text'; defaultValue?: string }
  | { type: 'boolean'; defaultValue?: boolean }
  | { type: 'color'; defaultValue?: string }
  | { type: 'select'; options: readonly string[]; defaultValue?: string }
  | { type: 'path' };

/**
 * A field as a table declares it: `required` when the props type requires
 * the prop, so the editor never offers to remove it.
 */
export type PrimitivePropertyField = PrimitiveFieldType & { required?: boolean };

type PropKey<P> = Extract<keyof P, string>;

/** The keys of `P` that are not optional. */
export type RequiredKeys<P> = { [K in keyof P]-?: {} extends Pick<P, K> ? never : K }[keyof P];

/** The field of prop `K`: declared `required` exactly when `P` requires `K`. */
type FieldOf<P, K extends keyof P> = K extends RequiredKeys<P>
  ? PrimitiveFieldType & { required: true }
  : PrimitiveFieldType & { required?: false };

/** The props that hold the box, for the editor's resize handles. */
export interface PrimitiveSize<P> {
  width?: PropKey<P>;
  height?: PropKey<P>;
  radius?: PropKey<P>;
}

/** A primitive's sections, in order, each its props in order. */
export interface PrimitivePropertyTable<P> {
  size?: PrimitiveSize<P>;
  groups: Record<string, { [K in PropKey<P>]?: FieldOf<P, K> }>;
}

/** `table`, checked against the props type `P`. */
export function propertyTable<P>(table: PrimitivePropertyTable<P>): PrimitivePropertyTable<P> {
  return table;
}

/** How far a number may go, and how much one pixel of a scrub changes it. */
export interface NumberRange {
  min?: number;
  max?: number;
  step?: number;
  /** Only whole numbers (a polygon's `sides`); a `step` of 1 does not imply it. */
  integer?: boolean;
}

export function num(defaultValue?: number, { min, max, step, integer }: NumberRange = {}): PrimitiveFieldType {
  return {
    type: 'number',
    ...(defaultValue === undefined ? {} : { defaultValue }),
    ...(min === undefined ? {} : { min }),
    ...(max === undefined ? {} : { max }),
    ...(step === undefined ? {} : { step }),
    ...(integer === true ? { integer } : {}),
  };
}

export function str(defaultValue?: string): PrimitiveFieldType {
  return defaultValue === undefined ? { type: 'string' } : { type: 'string', defaultValue };
}

/** A `<Text>`'s children. */
export function text(): PrimitiveFieldType {
  return { type: 'text' };
}

export function bool(defaultValue?: boolean): PrimitiveFieldType {
  return defaultValue === undefined ? { type: 'boolean' } : { type: 'boolean', defaultValue };
}

export function color(defaultValue?: string): PrimitiveFieldType {
  return defaultValue === undefined ? { type: 'color' } : { type: 'color', defaultValue };
}

export function select(options: readonly string[], defaultValue?: string): PrimitiveFieldType {
  return defaultValue === undefined ? { type: 'select', options } : { type: 'select', options, defaultValue };
}

/** A file path, as an `<Image>`'s `src`. */
export function path(): PrimitiveFieldType {
  return { type: 'path' };
}

/** `field` for a prop the props type requires: the editor never removes it. */
export function required<F extends PrimitiveFieldType>(field: F): F & { required: true } {
  return { ...field, required: true };
}

const tables = new Map<string, PrimitivePropertyTable<any>>();

/**
 * Registers `table` for the primitive `name`: the component's name, as a
 * source chain reports it.
 */
export function registerPrimitiveProperties(name: string, table: PrimitivePropertyTable<any>): void {
  tables.set(name, table);
}

/** Ordered for the wire: object key order is lost in Rust's maps. */
export interface WirePropertyTable {
  size?: { width?: string; height?: string; radius?: string };
  groups: { name: string; fields: { key: string; field: PrimitivePropertyField }[] }[];
}

/** Every registered table, keyed by primitive name. */
export function listPrimitiveProperties(): Record<string, WirePropertyTable> {
  const wire: Record<string, WirePropertyTable> = {};
  for (const [name, table] of tables) {
    wire[name] = {
      ...(table.size === undefined ? {} : { size: table.size }),
      groups: Object.entries(table.groups).map(([group, fields]) => ({
        name: group,
        fields: Object.entries(fields as Record<string, PrimitivePropertyField>).map(([key, field]) => ({ key, field })),
      })),
    };
  }
  return wire;
}
