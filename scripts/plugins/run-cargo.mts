// Run one package at a time so each codec links its own native FFmpeg build.
// `nextest` runs `cargo nextest run`. Each run writes its nextest profile's
// JUnit report (when the profile has one) to the same junit.xml, so this
// renames it to junit-<package>.xml beside it before the next package runs.
import { spawnSync } from 'node:child_process';
import { existsSync, renameSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { FFMPEG_PACKAGES, FFMPEG_X264_PACKAGE, ffmpegEnvironment } from './ffmpeg.mts';

const usage = `Usage: node scripts/plugins/run-cargo.mts <workspace> <command> -p <package> [-p <package> ...] [Cargo args...]
       node scripts/plugins/run-cargo.mts -h | --help

Run a Cargo command once per package. Use nextest for cargo nextest run.
Additional arguments are forwarded to Cargo.

Example:
  node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg --locked`;

function usageError(message: string): never {
  console.error(`run-cargo.mts: ${message}\nRun node scripts/plugins/run-cargo.mts --help for usage.`);
  process.exit(2);
}

const [workspace, command, ...args] = process.argv.slice(2);
if ((workspace === '--help' || workspace === '-h') && command === undefined) {
  console.log(usage);
  process.exit(0);
}
if (workspace?.startsWith('-')) usageError(`unexpected option ${JSON.stringify(workspace)}; expected a workspace directory`);
if (!workspace || !command) usageError('expected a workspace directory and Cargo command');
if (command.startsWith('-')) usageError(`unexpected option ${JSON.stringify(command)}; expected a Cargo command`);
const packages: string[] = [];
const rest: string[] = [];
for (let index = 0; index < args.length; index++) {
  if (args[index] === '-p') {
    const name = args[++index];
    if (!name || name.startsWith('-')) usageError('-p needs a package name');
    packages.push(name);
  } else rest.push(args[index]);
}
if (packages.length === 0) usageError('expected at least one -p <package>');
const nextest = command === 'nextest';
// Cross-plugin tests need independently linked sibling programs.
packages.sort((a, b) => Number(a === FFMPEG_X264_PACKAGE) - Number(b === FFMPEG_X264_PACKAGE));
// Only build flags belong on prerequisite builds, not test filters or libtest args.
// Under nextest, --profile (-P) is nextest's and --cargo-profile is Cargo's.
const buildFlags: string[] = [];
let nextestProfile = process.env.NEXTEST_PROFILE ?? 'default';
let targetDir = process.env.CARGO_TARGET_DIR ?? 'target';
for (let index = 0; index < rest.length && rest[index] !== '--'; index++) {
  const [flag, inline] = rest[index].startsWith('--') ? rest[index].split(/=(.*)/s, 2) : [rest[index], undefined];
  const takesValue = ['--target', '--profile', '--cargo-profile', '-P', '--target-dir', '--config', '--jobs', '-j'];
  if (['--release', '--locked', '--offline', '--frozen'].includes(flag)) {
    buildFlags.push(flag);
    continue;
  }
  if (!takesValue.includes(flag)) continue;
  const value = inline ?? rest[++index];
  if (value === undefined) usageError(`${flag} needs a value`);
  if (nextest && (flag === '--profile' || flag === '-P')) nextestProfile = value;
  else if (flag === '--cargo-profile') buildFlags.push('--profile', value);
  else {
    if (flag === '--target-dir') targetDir = value;
    buildFlags.push(flag, value);
  }
}
// .config/nextest.toml names the report junit.xml.
const junit = resolve(workspace, targetDir, 'nextest', nextestProfile, 'junit.xml');
const failures = new Set<string>();
function run(name: string, action: string, flags: string[]): boolean {
  const subcommand = action === 'nextest' ? ['nextest', 'run'] : [action];
  const result = spawnSync(process.env.CARGO ?? 'cargo', [...subcommand, '-p', name, ...flags], {
    cwd: resolve(workspace), env: ffmpegEnvironment(name), stdio: 'inherit',
  });
  if (result.error) console.error(`${name}: ${result.error.message}`);
  if (result.error || result.status !== 0) {
    failures.add(name);
    return false;
  }
  return true;
}
for (const name of packages) {
  // These tests start the two FFmpeg programs as sibling processes, each
  // linked against its own build.
  const crossPluginTests = name === FFMPEG_X264_PACKAGE
    || (name === 'celesta-codec-avfoundation' && process.platform === 'darwin')
    || (name === 'celesta-codec-mediafoundation' && process.platform === 'win32');
  if (crossPluginTests && (command === 'test' || nextest)) {
    let ready = true;
    for (const dependency of FFMPEG_PACKAGES) {
      if (!run(dependency, 'build', buildFlags.includes('--locked') ? buildFlags : ['--locked', ...buildFlags])) ready = false;
    }
    if (!ready) {
      console.error(`${name}: prerequisite build failed; continuing with other packages`);
      failures.add(name);
      continue;
    }
  }
  const report = junit.replace(/junit\.xml$/, `junit-${name}.xml`);
  // A report left from an earlier run must not stand for this one.
  if (nextest) for (const path of [junit, report]) rmSync(path, { force: true });
  run(name, command, rest);
  if (nextest && existsSync(junit)) renameSync(junit, report);
}
if (failures.size > 0) {
  console.error(`Cargo failed for: ${[...failures].join(', ')}`);
  process.exit(1);
}
