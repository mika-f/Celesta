import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';
import { ffmpegEnvironment } from '../ffmpeg.mts';

describe('native FFmpeg selection', () => {
  const environment = {
    PKG_CONFIG_PATH: '/lgpl/lib/pkgconfig', VCPKG_ROOT: '/lgpl', VCPKGRS_TRIPLET: 'base',
    FFMPEG_X264_PKG_CONFIG_PATH: '/gpl/lib/pkgconfig', FFMPEG_X264_VCPKG_ROOT: '/gpl',
    FFMPEG_X264_VCPKGRS_TRIPLET: 'gpl', PATH: '/bin',
  };
  it('keeps unrelated packages unchanged', () => {
    expect(ffmpegEnvironment('celesta-plugin-vrm', environment)).toEqual(environment);
  });
  it('keeps the standard package on LGPL libraries and fingerprints its native search path', () => {
    expect(ffmpegEnvironment('celesta-codec-ffmpeg', environment)).toEqual({
      ...environment, CARGO_ENCODED_RUSTFLAGS: '-L\x1fnative=/lgpl/installed/base/lib',
    });
  });
  it.each(['OPUS', 'VPX'])('selects the %s prefix independently', (variant) => {
    const selected = ffmpegEnvironment(`celesta-codec-ffmpeg-${variant.toLowerCase()}`, {
      ...environment, [`FFMPEG_${variant}_PKG_CONFIG_PATH`]: `/${variant}/lib/pkgconfig`,
      [`FFMPEG_${variant}_VCPKG_ROOT`]: `/${variant}`,
    });
    expect(selected.PKG_CONFIG_PATH).toBe(`/${variant}/lib/pkgconfig`);
    expect(selected.VCPKG_ROOT).toBe(`/${variant}`);
    expect(selected.CARGO_ENCODED_RUSTFLAGS).toBe(`-L\x1fnative=/${variant}/installed/base/lib`);
  });
  it('switches only the GPL process, without mutating the caller', () => {
    expect(ffmpegEnvironment('celesta-codec-ffmpeg-x264', environment)).toMatchObject({
      PKG_CONFIG_PATH: '/gpl/lib/pkgconfig', VCPKG_ROOT: '/gpl', VCPKGRS_TRIPLET: 'gpl', PATH: '/bin',
      CARGO_ENCODED_RUSTFLAGS: '-L\x1fnative=/gpl/installed/gpl/lib',
    });
    expect(environment.PKG_CONFIG_PATH).toBe('/lgpl/lib/pkgconfig');
  });
  it('preserves encoded flags and spaces, with Cargo precedence over RUSTFLAGS', () => {
    const selected = ffmpegEnvironment('celesta-codec-ffmpeg-x264', {
      ...environment, RUSTFLAGS: '--cfg ignored', CARGO_ENCODED_RUSTFLAGS: '--cfg\x1fexisting',
      FFMPEG_X264_VCPKG_ROOT: 'C:/native libraries/x264',
    });
    expect(selected.CARGO_ENCODED_RUSTFLAGS?.split('\x1f')).toEqual([
      '--cfg', 'existing', '-L', 'native=C:/native libraries/x264/installed/gpl/lib',
    ]);
  });
  it('preserves ordinary Rust flags when no encoded flags are supplied', () => {
    const selected = ffmpegEnvironment('celesta-codec-ffmpeg', { ...environment, RUSTFLAGS: '-C debuginfo=1' });
    expect(selected.CARGO_ENCODED_RUSTFLAGS?.split('\x1f')).toEqual([
      '-C', 'debuginfo=1', '-L', 'native=/lgpl/installed/base/lib',
    ]);
  });
  it('lets pkg-config builds use their existing environment tracking', () => {
    const environment = { PKG_CONFIG_PATH: '/native/lib/pkgconfig', RUSTFLAGS: '-C debuginfo=1' };
    expect(ffmpegEnvironment('celesta-codec-ffmpeg', environment)).toEqual(environment);
  });

  it.each([false, true])('rebuilds an untracked native dependency in one target directory (explicit target: %s)', (explicitTarget) => {
    const tree = realpathSync(mkdtempSync(join(tmpdir(), 'ffmpeg native cache ')));
    const cargo = process.env.CARGO ?? 'cargo';
    try {
      mkdirSync(join(tree, 'src'));
      writeFileSync(join(tree, 'Cargo.toml'), '[package]\nname = "native-probe"\nversion = "0.1.0"\nedition = "2021"\n');
      // Model vcpkg-rs: the build script reads its native root without emitting
      // rerun-if-env-changed. Switching only VCPKG_ROOT would reuse its answer.
      writeFileSync(join(tree, 'build.rs'), 'fn main() { println!("cargo:rustc-env=NATIVE_ROOT={}", std::env::var("VCPKG_ROOT").unwrap()); }\n');
      writeFileSync(join(tree, 'src/main.rs'), 'fn main() { println!("{}", env!("NATIVE_ROOT")); }\n');
      const environment = {
        ...process.env, CARGO_TARGET_DIR: join(tree, 'target'), CARGO_HOME: join(tree, 'cargo-home'),
        // Exercise Cargo's cache, independent of a developer's compiler cache.
        RUSTC_WRAPPER: '', RUSTC_WORKSPACE_WRAPPER: '',
        VCPKG_ROOT: join(tree, 'standard root'),
        FFMPEG_OPUS_VCPKG_ROOT: join(tree, 'opus root'),
        FFMPEG_VPX_VCPKG_ROOT: join(tree, 'vpx root'),
        FFMPEG_X264_VCPKG_ROOT: join(tree, 'x264 root'),
      };
      const extra: string[] = [];
      if (explicitTarget) {
        const compiler = spawnSync(process.env.RUSTC ?? 'rustc', ['-vV'], { encoding: 'utf8' });
        expect(compiler.status, compiler.stderr).toBe(0);
        const host = /^host: (.+)$/m.exec(compiler.stdout)?.[1];
        expect(host).toBeDefined();
        extra.push('--target', host!.trim());
      }
      for (const [variant, nativeRoot] of [
        ['', environment.VCPKG_ROOT], ['-opus', environment.FFMPEG_OPUS_VCPKG_ROOT],
        ['-vpx', environment.FFMPEG_VPX_VCPKG_ROOT], ['-x264', environment.FFMPEG_X264_VCPKG_ROOT],
        ['', environment.VCPKG_ROOT],
      ]) {
        const result = spawnSync(cargo, ['run', '--quiet', '--offline', ...extra], {
          cwd: tree, env: ffmpegEnvironment(`celesta-codec-ffmpeg${variant}`, environment), encoding: 'utf8',
        });
        expect(result.status, result.stderr).toBe(0);
        expect(result.stdout.trim()).toBe(nativeRoot);
      }
    } finally {
      rmSync(tree, { recursive: true, force: true });
    }
  }, 60_000);
});
