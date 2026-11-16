import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'vitest';

import { EASING_FACTORY_NAMES } from '../src/animation.js';
import { Easings } from '../src/index.js';

// Keyframe easings are evaluated in `crates/composition/src/animation.rs`;
// this fixture pins that evaluator.
interface Fixture {
  cases: {
    name: string;
    animation: number | { type: 'keyframes'; keyframes: { time: { value: number; timescale: number }; value: number; easing?: string }[] };
    samples: { time: { value: number; timescale: number }; value: number }[];
  }[];
}
const fixture: Fixture = JSON.parse(
  readFileSync(new URL('../../../crates/composition/tests/fixtures/keyframe-evaluation.json', import.meta.url), 'utf8'),
);

const camelCase = (name: string) => name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());

test('every keyframe easing is the Easings curve of the same name', () => {
  // The factories build curves; keyframe easings are the named curves only.
  const curves: Record<string, (t: number) => number> = Object.fromEntries(
    Object.entries(Easings).filter(([name]) => !(EASING_FACTORY_NAMES as readonly string[]).includes(name)),
  ) as Record<string, (t: number) => number>;
  const names = new Set(Object.keys(curves));
  const checked: string[] = [];
  for (const { name, animation, samples } of fixture.cases) {
    const curve = names.has(camelCase(name)) ? curves[camelCase(name)] : undefined;
    // The 0 → 1 over one second cases; the others test keyframe timing, not a curve.
    if (!curve || typeof animation === 'number' || animation.keyframes.length !== 2) continue;
    const [first, last] = animation.keyframes;
    if (last.easing !== name || first.value !== 0 || last.value !== 1) continue;
    for (const sample of samples) {
      const t = sample.time.value / sample.time.timescale;
      if (t > 1) continue;
      assert.ok(Math.abs(curve(t) - sample.value) <= 1e-9, `${name} at ${t}: Easings ${curve(t)}, keyframe ${sample.value}`);
    }
    checked.push(name);
  }
  assert.equal(checked.length, names.size);
});

test('easeIn, easeOut, and easeInOut are the quadratic curves', () => {
  for (let step = 0; step <= 20; step += 1) {
    const t = step / 20;
    // Written out twice in animation.ts, so equal up to rounding.
    assert.ok(Math.abs(Easings.easeIn(t) - Easings.easeInQuad(t)) <= 1e-12);
    assert.ok(Math.abs(Easings.easeOut(t) - Easings.easeOutQuad(t)) <= 1e-12);
    assert.ok(Math.abs(Easings.easeInOut(t) - Easings.easeInOutQuad(t)) <= 1e-12);
  }
  assert.equal(Easings.easeInOut(0.25), 0.125);
  assert.equal(Easings.easeInOut(0.75), 0.875);
});
