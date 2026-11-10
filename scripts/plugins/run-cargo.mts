// Run one package at a time so each codec links its own native FFmpeg build.
// Usage: run-cargo.mts <workspace> <test|clippy|build> -p <package>... [Cargo args...]
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { ffmpegEnvironment } from './ffmpeg.mts';
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
// The GPL process tests use the default process binary built by its own tests.
packages.sort((a, b) => Number(a === 'celesta-codec-ffmpeg-x264') - Number(b === 'celesta-codec-ffmpeg-x264'));
for (const name of packages) {
  if (name === 'celesta-codec-ffmpeg-x264' && command === 'test') {
    for (const dependency of ['celesta-codec-ffmpeg', 'celesta-codec-ffmpeg-opus', 'celesta-codec-ffmpeg-vpx']) {
      const built = spawnSync(process.env.CARGO ?? 'cargo', ['build', '-p', dependency, '--locked'], {
        cwd: resolve(workspace), env: ffmpegEnvironment(dependency), stdio: 'inherit',
      });
      if (built.error) throw built.error;
      if (built.status !== 0) process.exit(built.status ?? 1);
    }
  }
  const result = spawnSync(process.env.CARGO ?? 'cargo', [command, '-p', name, ...rest], {
    cwd: resolve(workspace), env: ffmpegEnvironment(name), stdio: 'inherit',
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
