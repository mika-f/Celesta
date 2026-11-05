import { describe, expect, it } from 'vitest';

import { skippedTests } from '../libtest.mts';

// Cargo's status words, colored as it prints them to a terminal or to CI.
const running = '\x1b[1m\x1b[92m     Running\x1b[0m';
const docTests = '\x1b[1m\x1b[92m   Doc-tests\x1b[0m';

// `cargo test -- --show-output`: a unit test binary without tests, an
// integration test binary whose passing tests print their output under
// `---- <test> stdout ----` headers, and a `harness = false` binary that
// prints its own lines.
const log = [
  `${running} unittests src/lib.rs (target/debug/deps/a-7bc534db127be6c4)`,
  'running 0 tests',
  `${running} tests/gpu.rs (target/debug/deps/gpu-777820c74e284fab)`,
  'running 2 tests',
  'successes:',
  '',
  '---- t stdout ----',
  'skipping GPU test because none',
  'other',
  '',
  '---- v stdout ----',
  'skipping GPU test because none',
  '',
  'successes:',
  '    t',
  '    u',
  '    v',
  `${running} tests/process.rs (target/debug/deps/process-cf4fb80f53236909)`,
  'skipping plain: no gpu',
  'test the_process_answers_as_the_provider_does ... ok',
  `${docTests} a`,
  '',
].join('\n');

const expected = new Map([
  ['skipping GPU test because none', ['tests/gpu.rs t', 'tests/gpu.rs v']],
  ['skipping plain: no gpu', ['tests/process.rs']],
]);

describe('skippedTests', () => {
  it('names a skipped test by its binary and a harness-less binary by its label', () => {
    expect(skippedTests(log)).toEqual(expected);
  });

  it('reads Windows line endings', () => {
    expect(skippedTests(log.replaceAll('\n', '\r\n'))).toEqual(expected);
  });

  it('names a doctest binary by its crate', () => {
    const doctests = [`${docTests} a`, '---- src/lib.rs - f (line 3) stdout ----', 'skipping no font', ''].join('\n');
    expect(skippedTests(doctests)).toEqual(new Map([['skipping no font', ['doctests a src/lib.rs - f (line 3)']]]));
  });

  it('gives nothing for a log without skipping lines', () => {
    expect(skippedTests(log.replaceAll('skipping', 'ran'))).toEqual(new Map());
  });

  it('reads a line starting with skipped as a skip', () => {
    const skipped = [
      `${running} tests/samples.rs (target/debug/deps/samples-1)`,
      '---- loads_samples stdout ----',
      'skipped: CELESTA_VRM_SAMPLES_DIR is not set',
      'skippedness is not a skip',
      '',
    ].join('\n');
    expect(skippedTests(skipped)).toEqual(
      new Map([['skipped: CELESTA_VRM_SAMPLES_DIR is not set', ['tests/samples.rs loads_samples']]]),
    );
  });

  it("names a Windows binary with / separators, as Cargo's label on other systems", () => {
    const windows = [
      `${running} unittests src\\lib.rs (target\\debug\\deps\\a-7bc534db127be6c4.exe)`,
      'skipping no gpu',
      '',
    ].join('\r\n');
    expect(skippedTests(windows)).toEqual(new Map([['skipping no gpu', ['unittests src/lib.rs']]]));
  });

  it("ignores Cargo's own lines before the first binary", () => {
    const before = ['skipping before any binary', `${running} tests/gpu.rs (target/debug/deps/gpu-1)`, ''].join('\n');
    expect(skippedTests(before)).toEqual(new Map());
  });

  it("names a failing test's skip line by the test, and a line after the failures list by the binary", () => {
    const failures = [
      `${running} tests/gpu.rs (target/debug/deps/gpu-1)`,
      'failures:',
      '',
      '---- t stdout ----',
      'skipping inside t',
      '',
      'failures:',
      '    t',
      'skipping after the list',
      '',
    ].join('\n');
    expect(skippedTests(failures)).toEqual(
      new Map([
        ['skipping inside t', ['tests/gpu.rs t']],
        ['skipping after the list', ['tests/gpu.rs']],
      ]),
    );
  });

  it("ends a test's output at a ---- line that is not a stdout header", () => {
    const closed = [
      `${running} tests/gpu.rs (target/debug/deps/gpu-1)`,
      '---- t stdout ----',
      'skipping inside t',
      '---- t stderr ----',
      'skipping outside t',
      '',
    ].join('\n');
    expect(skippedTests(closed)).toEqual(
      new Map([
        ['skipping inside t', ['tests/gpu.rs t']],
        ['skipping outside t', ['tests/gpu.rs']],
      ]),
    );
  });
});
