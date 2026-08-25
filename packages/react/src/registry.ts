import type { ReactNode } from 'react';

import type { JsonValue } from './generated/serde_json/JsonValue';

// project.json can place a registered component instance on the timeline
// (`TimelineContent::Component { component, props }`). mikan-evaluator
// (Rust) has no registry of its own — it always evaluates that content to
// `LayerContent::MissingComponent { component, props }` — so resolving the
// name to an actual component is entirely this module's job, done by
// project-runtime.ts's `<ProjectTimeline />` when it walks the layers Rust
// evaluated. This registry is process-global: each `mikan-react-render`
// process handles exactly one entry for its whole lifetime, so there is no
// cross-entry state to worry about.

export type ComponentDefinition<Props extends Record<string, JsonValue> = Record<string, JsonValue>> = (
  props: Props,
) => ReactNode;

const registry = new Map<string, ComponentDefinition>();

/**
 * Registers a component under `name` so `<ProjectTimeline />` can resolve a
 * project.json `{"type": "component", "component": name, "props": {...}}`
 * timeline item to it. Call this at module scope in the entry (or a module
 * it imports) so registration happens before any frame is rendered.
 */
export function registerComponent<Props extends Record<string, JsonValue>>(
  name: string,
  component: ComponentDefinition<Props>,
): void {
  registry.set(name, component as ComponentDefinition);
}

export function resolveComponent(name: string): ComponentDefinition | undefined {
  return registry.get(name);
}
