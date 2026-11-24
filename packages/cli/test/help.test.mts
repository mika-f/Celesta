import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const { version } = createRequire(import.meta.url)('../package.json');

test.each(['--help', '-h', '--version', '-V'])('%s does not start the React runtime', (flag) => {
  const result = spawnSync(process.execPath, [cli, flag], { encoding: 'utf8', timeout: 5000 });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, '');
  if (flag === '--help' || flag === '-h') {
    assert.match(result.stdout, /^celesta-react-render/);
    assert.match(result.stdout, /JSON messages over stdin\/stdout/);
    assert.match(result.stdout, /celesta-exporter --help/);
  } else {
    assert.equal(result.stdout, `celesta-react-render ${version}\n`);
  }
});
