import type { ComponentPropertyField } from './registry';

// The GUI editor's Inspector can only show editable fields for a project's
// `properties` map (`Record<string, JsonValue>`) if the entry declares what
// those fields are. `defineProjectProperties` declares exactly that, once at
// module scope, using the same field shape a registered component's schema
// uses — the two concepts differ in where their values live (project-level
// vs per timeline item), not in how each field is edited or displayed.
export type ProjectPropertyField = ComponentPropertyField;

export type ProjectPropertySchema = Record<string, ProjectPropertyField>;

let declared: ProjectPropertySchema | undefined;

/**
 * Declares the entry's GUI-editable project properties. Call this at module
 * scope so it has run before the CLI's startup `Ready` message is sent — the
 * schema rides that handshake to the editor and plays no part in rendering
 * or evaluation here; values themselves live in the project file
 * (`Project.properties`) and are read back with `useProjectProperty`.
 */
export function defineProjectProperties(schema: ProjectPropertySchema): void {
  declared = schema;
}

/** The declared project property schema, or `undefined` when none was declared. */
export function listProjectProperties(): ProjectPropertySchema | undefined {
  return declared;
}
