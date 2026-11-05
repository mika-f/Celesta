import { describe, expect, it } from 'vitest';

import { skippedTests } from '../libtest.mjs';

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
});
