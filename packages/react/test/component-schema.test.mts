import assert from 'node:assert/strict';

import { test } from 'vitest';

import { getComponentSchema, registerComponent } from '../src/index.js';
import type { ComponentPropertySchema } from '../src/index.js';

type BadgeProps = { caption: string; count: number; wide?: boolean };

function Badge(_props: BadgeProps) {
  return null;
}

test('a schema keeps `required` as declared', () => {
  registerComponent('RequiredBadge', Badge, {
    caption: { type: 'string', defaultValue: '', required: true },
    count: { type: 'number', defaultValue: 0, required: true },
    wide: { type: 'boolean', defaultValue: false },
  });
  assert.deepEqual(getComponentSchema('RequiredBadge'), {
    caption: { type: 'string', defaultValue: '', required: true },
    count: { type: 'number', defaultValue: 0, required: true },
    wide: { type: 'boolean', defaultValue: false },
  });
});

// Type test: compiled by `tsc -b test`, never run. A prop the props type
// requires must be declared `required`, and an optional one must not be.
export function requiredMatchesThePropsType() {
  const missing: ComponentPropertySchema<BadgeProps> = {
    caption: { type: 'string', defaultValue: '', required: true },
    // @ts-expect-error `count` is required by `BadgeProps`.
    count: { type: 'number', defaultValue: 0 },
  };
  const extra: ComponentPropertySchema<BadgeProps> = {
    caption: { type: 'string', defaultValue: '', required: true },
    count: { type: 'number', defaultValue: 0, required: true },
    // @ts-expect-error `wide` is optional.
    wide: { type: 'boolean', defaultValue: false, required: true },
  };
  registerComponent('MissingBadge', Badge, {
    caption: { type: 'string', defaultValue: '', required: true },
    // @ts-expect-error `count` is required by `BadgeProps`.
    count: { type: 'number', defaultValue: 0 },
  });
  // A schema not typed by its props may declare either.
  const untyped: ComponentPropertySchema = {
    count: { type: 'number', defaultValue: 0, required: true },
    wide: { type: 'boolean', defaultValue: false },
  };
  return [missing, extra, untyped];
}
