import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/** Renders frame 0 of a composition whose body is `body`, as the CLI's output lines. */
function render(body) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-clip-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Group, Rect } from '@celesta/react';\n` +
      `export default function Root() {\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={1}>\n` +
      `      ${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a group clip reaches the scene, with x, y, and cornerRadius defaulting to 0', () => {
  const [, frame] = render(
    `<Group x={10} y={20} clip={{ width: 120, height: 40 }}><Rect width={300} height={200} fill="#FF0000" /></Group>` +
      `<Group clip={{ x: 5, y: 6, width: 7, height: 8, cornerRadius: 3 }} />`,
  );
  const [first, second] = frame.scene.layers;
  assert.equal(first.content.type, 'group');
  assert.deepEqual(first.content.clip, { x: 0, y: 0, width: 120, height: 40, cornerRadius: 0 });
  assert.deepEqual(first.transform.position, { x: 10, y: 20 });
  assert.deepEqual(second.content.clip, { x: 5, y: 6, width: 7, height: 8, cornerRadius: 3 });
});

test('a group without a clip has no clip in the scene', () => {
  const [, frame] = render(`<Group><Rect width={10} height={10} fill="#FF0000" /></Group>`);
  const [group] = frame.scene.layers;
  assert.equal(group.content.type, 'group');
  assert.equal('clip' in group.content, false);
});

test('a group clip needs a finite width and height', () => {
  for (const clip of ['{{ height: 10 }}', '{{ width: 10 }}', '{{ width: NaN, height: 10 }}', '{{ width: "10", height: 10 }}']) {
    const [, frame] = render(`<Group clip=${clip} />`);
    assert.match(frame.error ?? '', /requires finite `width` and `height`/, clip);
  }
});
