import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', 'report-skipped-tests.mjs');

describe('report-skipped-tests.mjs on a nextest JUnit report', () => {
  let folder: string;

  beforeEach(() => {
    folder = mkdtempSync(join(tmpdir(), 'celesta-skipped-'));
  });

  afterEach(() => {
    rmSync(folder, { recursive: true, force: true });
  });

  function report(xml: string) {
    const path = join(folder, 'junit.xml');
    writeFileSync(path, xml);
    // Plain lines, as outside GitHub Actions, where the tests also run.
    const env = { ...process.env };
    delete env.GITHUB_ACTIONS;
    delete env.GITHUB_STEP_SUMMARY;
    const result = spawnSync(process.execPath, [script, path], { encoding: 'utf8', env });
    expect(result.status).toBe(0);
    return result.stdout;
  }

  it('reads lines starting with skipping or skipped, and no other', () => {
    const xml = [
      '<testsuites><testsuite name="a">',
      '<testcase classname="a::gpu" name="draws"><system-out>skipping GPU test because none\n</system-out></testcase>',
      '<testcase classname="a::samples" name="loads"><system-out>skipped: CELESTA_VRM_SAMPLES_DIR is not set\n</system-out></testcase>',
      '<testcase classname="a::other" name="runs"><system-out>skippedness is not a skip\n</system-out></testcase>',
      '<testcase classname="a::bare" name="passes"/>',
      '</testsuite></testsuites>',
    ].join('\n');
    expect(report(xml)).toBe(
      ['1 test: skipping GPU test because none', '1 test: skipped: CELESTA_VRM_SAMPLES_DIR is not set', ''].join('\n'),
    );
  });

  it('says so when no test skipped itself', () => {
    expect(report('<testsuites><testsuite name="a"><testcase classname="a" name="b"/></testsuite></testsuites>')).toBe(
      'No tests skipped themselves.\n',
    );
  });
});
