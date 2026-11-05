// Report the tests that skipped themselves in a nextest JUnit report or in a
// `cargo test -- --show-output` log. The standard test runner cannot mark a
// test skipped, so a test that cannot run here (no GPU adapter, no Node.js, no
// color emoji font, ...) prints a line starting with "skipping" or "skipped"
// and passes.
// nextest keeps a passing test's output only in the report
// (`store-success-output` in .config/nextest.toml); `cargo test` prints it
// with `--show-output` (the plugin job, whose `harness = false` targets
// nextest cannot run), and scripts/plugins/libtest.mts reads that log.
//
// In GitHub Actions each distinct reason becomes one warning annotation with
// its test count, and the job summary lists every skipped test.
//
//   node scripts/report-skipped-tests.mts target/nextest/ci/junit.xml
//   node scripts/report-skipped-tests.mts "$RUNNER_TEMP/cargo-test.log"
import { appendFileSync, existsSync, readFileSync } from 'node:fs';

import { SKIP_LINE, skippedTests } from './plugins/libtest.mts';

const [report] = process.argv.slice(2);
if (!report) {
  console.error('Usage: node scripts/report-skipped-tests.mts <junit.xml | cargo-test.log>');
  process.exit(2);
}
if (!existsSync(report)) {
  // The test run stopped before writing it; its own error says why.
  console.log(`No report at ${report}.`);
  process.exit(0);
}

const unescape = (text: string) =>
  text.replace(/&(lt|gt|quot|apos|amp|#x?[0-9a-fA-F]+);/g, (entity, name: string) => {
    switch (name) {
      case 'lt':
        return '<';
      case 'gt':
        return '>';
      case 'quot':
        return '"';
      case 'apos':
        return "'";
      case 'amp':
        return '&';
      default:
        return String.fromCodePoint(
          name[1] === 'x' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10),
        );
    }
  });

/** Skipped tests by reason in a nextest JUnit report. */
function junitSkippedTests(xml: string): Map<string, string[]> {
  const reasons = new Map<string, string[]>();
  for (const [, attributes, body] of xml.matchAll(/<testcase\b([^>]*?)(?:\/>|>([\s\S]*?)<\/testcase>)/g)) {
    if (!body) continue;
    const attribute = (key: string) => unescape(attributes.match(new RegExp(`\\b${key}="([^"]*)"`))?.[1] ?? '');
    const test = `${attribute('classname')} ${attribute('name')}`;
    for (const [, output] of body.matchAll(/<system-(?:out|err)>([\s\S]*?)<\/system-(?:out|err)>/g)) {
      for (const line of unescape(output).split(/\r?\n/)) {
        if (!SKIP_LINE.test(line)) continue;
        const tests = reasons.get(line) ?? [];
        if (!tests.includes(test)) tests.push(test);
        reasons.set(line, tests);
      }
    }
  }
  return reasons;
}

const text = readFileSync(report, 'utf8');
const reasons = report.endsWith('.xml') ? junitSkippedTests(text) : skippedTests(text);

if (reasons.size === 0) {
  console.log('No tests skipped themselves.');
  process.exit(0);
}

const escapeData = (text: string) => text.replace(/%/g, '%25').replace(/\r/g, '%0D').replace(/\n/g, '%0A');
const plural = (count: number) => `${count} test${count === 1 ? '' : 's'}`;
const sorted = [...reasons].sort(([, a], [, b]) => b.length - a.length);
for (const [reason, tests] of sorted) {
  if (process.env.GITHUB_ACTIONS === 'true') {
    console.log(`::warning title=Skipped ${plural(tests.length)}::${escapeData(reason)}`);
  } else {
    console.log(`${plural(tests.length)}: ${reason}`);
  }
}

if (process.env.GITHUB_STEP_SUMMARY) {
  const total = new Set(sorted.flatMap(([, tests]) => tests)).size;
  const html = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const lines = [`### Skipped tests (${total})`, ''];
  for (const [reason, tests] of sorted) {
    lines.push(`<details><summary>${plural(tests.length)}: <code>${html(reason)}</code></summary>`, '');
    for (const test of tests.sort()) lines.push(`- \`${test}\``);
    lines.push('', '</details>', '');
  }
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join('\n')}\n`);
}
