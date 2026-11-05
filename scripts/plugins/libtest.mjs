// The tests that skipped themselves in a `cargo test -- --show-output` log.
// A test that cannot run here prints a line starting with "skipping" or
// "skipped" and passes; `--show-output` prints a passing test's captured
// output under a `---- <test> stdout ----` header, and a `harness = false`
// binary prints its own lines right after Cargo's `Running` line for it.

/**
 * A line a test prints when it skips itself: `skipping ...` or `skipped: ...`.
 * scripts/report-skipped-tests.mjs reads a JUnit report's output with it too.
 */
export const SKIP_LINE = /^skipp(ing|ed)\b/;

/**
 * Skipped tests by reason line. A test is named `<binary label> <test>`
 * (`tests/mtoon.rs some_test`), and a line a binary printed outside any test's
 * output by the binary's label alone (`tests/process.rs`). The label is what
 * Cargo's `Running` line names (`unittests src/lib.rs`, `tests/gpu.rs`), with
 * Windows' `\` as `/`, or `doctests <crate>` for `Doc-tests <crate>`. Lines
 * before the first binary are Cargo's own and are not read.
 *
 * @param {string} log
 * @returns {Map<string, string[]>}
 */
export function skippedTests(log) {
  /** @type {Map<string, string[]>} */
  const reasons = new Map();
  /** @type {string | undefined} */
  let label;
  /** @type {string | undefined} */
  let test;
  for (const line of log.replace(/\x1b\[[0-9;]*m/g, '').split(/\r?\n/)) {
    const binary = /^\s*Running (.+) \(.+\)$/.exec(line);
    const doctests = /^\s*Doc-tests (\S+)$/.exec(line);
    if (binary || doctests) {
      label = binary ? binary[1].replaceAll('\\', '/') : `doctests ${doctests?.[1]}`;
      test = undefined;
      continue;
    }
    if (line.startsWith('----')) {
      test = /^---- (.+) stdout ----$/.exec(line)?.[1];
      continue;
    }
    if (line === 'successes:' || line === 'failures:') {
      test = undefined;
      continue;
    }
    if (label === undefined || !SKIP_LINE.test(line)) continue;
    const name = test === undefined ? label : `${label} ${test}`;
    const tests = reasons.get(line) ?? [];
    if (!tests.includes(name)) tests.push(name);
    reasons.set(line, tests);
  }
  return reasons;
}
