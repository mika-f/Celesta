import type { JsonValue } from './generated/serde_json/JsonValue';
import type { ComponentPropertyField } from './registry';

// The GUI editor's Inspector can only show editable fields for a project's
// `properties` map (`Record<string, JsonValue>`) if the entry declares what
// those fields are. `defineProjectProperties` declares exactly that, once at
// module scope, using the same field shape a registered component's schema
// uses — the two concepts differ in where their values live (project-level
// vs per timeline item), not in how each field is edited or displayed.
//
// The same schema also types the values a render receives from outside the
// source: `celesta-exporter --props/--props-file` and the companion
// project's `properties`. The CLI (cli.ts) validates those against it before
// `prepare()` runs and hands the result to `setProjectPropertyValues`.
export type ProjectPropertyField = ComponentPropertyField;

export type ProjectPropertySchema = Record<string, ProjectPropertyField>;

let declared: ProjectPropertySchema | undefined;

// Values given from outside the source, already validated; they win over a
// `<ProjectProvider>` and the schema defaults.
let inputValues: Record<string, JsonValue> = {};
// Schema defaults with `path` defaults made absolute; `undefined` reads the
// declared schema as-is (the browser runtime never resolves paths).
let resolvedDefaults: Record<string, JsonValue> | undefined;
// False between the CLI loading the entry and validating its inputs, so a
// module-scope read cannot see values that are not final yet.
let valuesReady = true;

/**
 * Declares the entry's project properties. Call this at module scope so it
 * has run before the CLI's startup `Ready` message is sent. The schema types
 * the values `--props`/`--props-file` and a companion project pass in,
 * supplies their defaults, and describes the Inspector rows.
 */
export function defineProjectProperties(schema: ProjectPropertySchema): void {
  declared = schema;
}

/** The declared project property schema, or `undefined` when none was declared. */
export function listProjectProperties(): ProjectPropertySchema | undefined {
  return declared;
}

/**
 * The value of project property `key` from `--props`, `--props-file` or the
 * companion project, else its declared default, else `defaultValue`. Unlike
 * `useProjectProperty` it is not a hook, so `prepare()` can read the values
 * a render will use — a data file path, or a count that sets the duration.
 * It does not see a `<ProjectProvider>`, which only exists inside the tree.
 */
export function getProjectProperty<T extends JsonValue = JsonValue>(key: string, defaultValue?: T): T {
  const value = inputProjectProperty(key) ?? declaredDefault(key) ?? defaultValue;
  if (value === undefined) {
    throw new Error(`project property "${key}" is not declared with defineProjectProperties() and has no default value`);
  }
  return value as T;
}

/** The validated outside value of `key`, if one was given. */
export function inputProjectProperty(key: string): JsonValue | undefined {
  if (!valuesReady) {
    throw new Error(
      `project property "${key}" was read at module scope; read it in prepare() or while rendering, after the inputs are validated`,
    );
  }
  return has(inputValues, key) ? inputValues[key] : undefined;
}

/** The declared default of `key`, if the schema has one. */
export function declaredDefault(key: string): JsonValue | undefined {
  if (resolvedDefaults && has(resolvedDefaults, key)) return resolvedDefaults[key];
  return declared && has(declared, key) ? declared[key].defaultValue : undefined;
}

function has(record: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(record, key);
}

/** Hides the values until `setProjectPropertyValues`; the CLI calls this before loading the entry. */
export function holdProjectPropertyValues(): void {
  valuesReady = false;
}

/** Installs the validated outside values and the resolved defaults. */
export function setProjectPropertyValues(values: Record<string, JsonValue>, defaults: Record<string, JsonValue>): void {
  inputValues = values;
  resolvedDefaults = defaults;
  valuesReady = true;
}
