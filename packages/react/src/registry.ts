import type { ReactNode } from 'react';

import type { JsonValue } from './generated/serde_json/JsonValue';

// project.json can place a registered component instance on the timeline
// (`TimelineContent::Component { component, props }`). celesta-evaluator
// (Rust) has no registry of its own — it always evaluates that content to
// `LayerContent::MissingComponent { component, props }` — so resolving the
// name to an actual component is entirely this module's job, done by
// project-runtime.ts's `<ProjectTimeline />` when it walks the layers Rust
// evaluated. This registry is process-global: each `celesta-react-render`
// process handles exactly one entry for its whole lifetime, so there is no
// cross-entry state to worry about.

export type ComponentDefinition<Props extends Record<string, JsonValue> = Record<string, JsonValue>> = (
  props: Props,
) => ReactNode;

// A registered component's props are opaque JSON to the GUI editor (Rust
// never sees `Props`, only `props: BTreeMap<String, serde_json::Value>` on
// the timeline item), so there is no way for the editor to know what
// fields exist, what type each one is, or how to present it in an
// Inspector without the entry telling it. A `ComponentPropertySchema`
// declares exactly that, per field, alongside the registration — it is
// metadata for a future GUI-side Inspector, not something this package's
// own rendering reads today.
export type ComponentPropertyField =
  | { type: 'string'; label?: string; defaultValue: string }
  | { type: 'number'; label?: string; defaultValue: number; min?: number; max?: number; step?: number }
  | { type: 'boolean'; label?: string; defaultValue: boolean }
  | { type: 'color'; label?: string; defaultValue: string }
  | { type: 'select'; label?: string; defaultValue: string; options: readonly string[] };

export type ComponentPropertySchema<Props extends Record<string, JsonValue> = Record<string, JsonValue>> = {
  [K in keyof Props]: ComponentPropertyField;
};

interface RegistryEntry {
  component: ComponentDefinition;
  schema: ComponentPropertySchema | undefined;
}

const registry = new Map<string, RegistryEntry>();

/**
 * Registers a component under `name` so `<ProjectTimeline />` can resolve a
 * project.json `{"type": "component", "component": name, "props": {...}}`
 * timeline item to it. Call this at module scope in the entry (or a module
 * it imports) so registration happens before any frame is rendered.
 *
 * `schema`, when given, declares each prop's type, default value, and
 * display hints for a future GUI Inspector — see
 * `getComponentSchema`/`ComponentPropertySchema`. It plays no part in
 * rendering or evaluation; a component with no schema resolves and renders
 * exactly as one with a schema does.
 */
export function registerComponent<Props extends Record<string, JsonValue>>(
  name: string,
  component: ComponentDefinition<Props>,
  schema?: ComponentPropertySchema<Props>,
): void {
  registry.set(name, { component: component as ComponentDefinition, schema: schema as ComponentPropertySchema | undefined });
}

export function resolveComponent(name: string): ComponentDefinition | undefined {
  return registry.get(name)?.component;
}

/**
 * Looks up the property schema a registered component declared, if any.
 * Returns `undefined` both when `name` was never registered and when it
 * was registered without a `schema` argument — callers that need to tell
 * those apart should check `resolveComponent(name)` first.
 */
export function getComponentSchema(name: string): ComponentPropertySchema | undefined {
  return registry.get(name)?.schema;
}

/**
 * Every registered component's schema, keyed by name — omitting components
 * registered without one. `cli.ts` sends this once, in the startup `Ready`
 * message (all `registerComponent()` calls have already run by then, since
 * they happen at module scope), so a GUI editor can discover an entry's
 * component schemas without knowing the component names up front.
 */
export function listComponentSchemas(): Record<string, ComponentPropertySchema> {
  const schemas: Record<string, ComponentPropertySchema> = {};
  for (const [name, entry] of registry) {
    if (entry.schema !== undefined) {
      schemas[name] = entry.schema;
    }
  }
  return schemas;
}
