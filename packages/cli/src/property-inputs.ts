import * as fs from 'node:fs';
import * as path from 'node:path';

import type { JsonValue, ProjectPropertyField, ProjectPropertySchema } from '@celesta/react';

// Validates the project property values a render receives from outside its
// source against the entry's `defineProjectProperties()` schema. Node-only
// (it resolves and checks paths), so it is imported by the CLI and never by
// the browser runtime.

/**
 * One source of values, as the Rust bridge sends them, lowest precedence
 * first: a companion project's `properties`, then `--props-file`, then
 * `--props`.
 */
export interface PropertyInputLayer {
  source: 'project' | 'propsFile' | 'props';
  /** The file the values came from; absent for `--props`. */
  file?: string;
  /** Where relative `path` values resolve from. */
  baseDir: string;
  values: Record<string, JsonValue>;
}

/** One rejected value, reported to the caller before anything renders. */
export interface PropertyIssue {
  /** The property key; empty when the issue concerns the whole source. */
  key: string;
  /** The option or file that supplied the value, e.g. `--props-file variant.json`. */
  source: string;
  message: string;
}

export interface ResolvedProperties {
  /** The winning outside value of every key that was given one. */
  values: Record<string, JsonValue>;
  /** Every declared default, `path` defaults made absolute against the entry's directory. */
  defaults: Record<string, JsonValue>;
  issues: PropertyIssue[];
}

const HEX_COLOR = /^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/;
const REMOTE_URL = /^https?:\/\//i;

/**
 * Merges `layers` (later wins) after checking each value against `schema`.
 * `--props`/`--props-file` are strict: every key must be declared. A
 * companion project's `properties` predate this check and may hold values
 * for a `<ProjectProvider>` or other tools, so its undeclared keys are
 * ignored; its declared ones are checked like the rest.
 */
export function resolvePropertyInputs(
  layers: readonly PropertyInputLayer[],
  schema: ProjectPropertySchema | undefined,
  entryDir: string,
): ResolvedProperties {
  // Prototype-free, so a key such as `__proto__` is stored as an own value.
  const values: Record<string, JsonValue> = Object.create(null);
  const issues: PropertyIssue[] = [];
  for (const layer of layers) {
    const source = describeSource(layer);
    const strict = layer.source !== 'project';
    if (!schema) {
      if (strict && Object.keys(layer.values).length > 0) {
        issues.push({
          key: '',
          source,
          message: 'the entry declares no project properties; call defineProjectProperties() to accept outside values',
        });
      }
      continue;
    }
    for (const [key, value] of Object.entries(layer.values)) {
      const field = Object.prototype.hasOwnProperty.call(schema, key) ? schema[key] : undefined;
      if (!field) {
        if (strict) {
          const declared = Object.keys(schema);
          issues.push({
            key,
            source,
            message: `not a declared project property (declared: ${declared.length > 0 ? declared.join(', ') : 'none'})`,
          });
        }
        continue;
      }
      const checked = checkValue(field, value, layer.baseDir);
      if ('error' in checked) {
        issues.push({ key, source, message: checked.error });
      } else {
        values[key] = checked.value;
      }
    }
  }

  // Defaults are resolved but not checked: a variant or a `<ProjectProvider>`
  // may supply the file, and `prepare()` reports a missing default itself.
  const defaults: Record<string, JsonValue> = Object.create(null);
  for (const [key, field] of Object.entries(schema ?? {})) {
    defaults[key] = field.type === 'path' ? resolvePath(field.defaultValue, entryDir) : field.defaultValue;
  }
  return { values, defaults, issues };
}

function describeSource(layer: PropertyInputLayer): string {
  switch (layer.source) {
    case 'props':
      return '--props';
    case 'propsFile':
      return `--props-file ${layer.file ?? ''}`.trimEnd();
    case 'project':
      return `--project ${layer.file ?? ''}`.trimEnd();
  }
}

function checkValue(field: ProjectPropertyField, value: JsonValue, baseDir: string): { value: JsonValue } | { error: string } {
  switch (field.type) {
    case 'string':
      return typeof value === 'string' ? { value } : expected('a string', value);
    case 'number':
      return typeof value === 'number' && Number.isFinite(value) ? { value } : expected('a finite number', value);
    case 'boolean':
      return typeof value === 'boolean' ? { value } : expected('true or false', value);
    case 'color':
      return typeof value === 'string' && HEX_COLOR.test(value) ? { value } : expected('a #RRGGBB or #RRGGBBAA color', value);
    case 'select':
      return typeof value === 'string' && field.options.includes(value)
        ? { value }
        : expected(`one of ${field.options.map((option) => JSON.stringify(option)).join(', ')}`, value);
    case 'path': {
      if (typeof value !== 'string') return expected('a file path or URL', value);
      // `""` means "no file", as an empty default does.
      if (value === '' || REMOTE_URL.test(value)) return { value };
      const resolved = resolvePath(value, baseDir);
      if (!isFile(resolved)) {
        return { error: `${resolved} is not an existing file` };
      }
      return { value: resolved };
    }
  }
}

function isFile(file: string): boolean {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

function expected(what: string, value: JsonValue): { error: string } {
  return { error: `expected ${what}, got ${JSON.stringify(value)}` };
}

/** Absolute local paths and URLs are kept; relative paths join `baseDir`. */
function resolvePath(value: string, baseDir: string): string {
  // An empty default means "no file"; keep it rather than resolving to `baseDir`.
  return value === '' || REMOTE_URL.test(value) ? value : path.resolve(baseDir, value);
}
