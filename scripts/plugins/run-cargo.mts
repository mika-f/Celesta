// Run one package at a time so each codec links its own native FFmpeg build.
// Usage: run-cargo.mts <workspace> <test|clippy|build> -p <package>... [Cargo args...]
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { FFMPEG_PACKAGES, FFMPEG_X264_PACKAGE, ffmpegEnvironment } from './ffmpeg.mts';
const [workspace, command, ...args] = process.argv.slice(2);
if (!workspace || !command) throw new Error('expected workspace and Cargo command');
const packages: string[] = [];
const rest: string[] = [];
for (let index = 0; index < args.length; index++) {
  if (args[index] === '-p') {
    const name = args[++index];
    if (!name) throw new Error('-p needs a package');
    packages.push(name);
  } else rest.push(args[index]);
}
if (packages.length === 0) throw new Error('expected at least one -p package');
// Cross-plugin tests need independently linked sibling programs.
packages.sort((a, b) => Number(a === FFMPEG_X264_PACKAGE) - Number(b === FFMPEG_X264_PACKAGE));
// Only build flags belong on prerequisite builds, not test filters or libtest args.
const buildFlags: string[] = [];
for (let index = 0; index < rest.length && rest[index] !== '--'; index++) {
  const flag = rest[index];
  if (['--release', '--locked', '--offline', '--frozen'].includes(flag)) buildFlags.push(flag);
  else if (['--target', '--profile', '--target-dir', '--config', '--jobs', '-j'].includes(flag)) {
    const value = rest[++index];
    if (value === undefined) throw new Error(`${flag} needs a value`);
    buildFlags.push(flag, value);
  } else if (/^--(target|profile|target-dir|config|jobs)=/.test(flag)) buildFlags.push(flag);
}
const failures = new Set<string>();
function run(name: string, action: string, flags: string[]): boolean {
  const result = spawnSync(process.env.CARGO ?? 'cargo', [action, '-p', name, ...flags], {
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
  if (crossPluginTests && command === 'test') {
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
  run(name, command, rest);
}
if (failures.size > 0) {
  console.error(`Cargo failed for: ${[...failures].join(', ')}`);
  process.exit(1);
}
