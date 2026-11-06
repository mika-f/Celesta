import { describe, expect, it } from 'vitest';

import { parseRustcCfg, rustPlatform } from '../platform.mts';

// Excerpts of `rustc --print cfg --target <triple>`, with lines that are not
// the OS or the architecture around them.
function cfg(os: string, arch: string): string {
  return [
    'debug_assertions',
    'panic="unwind"',
    `target_arch="${arch}"`,
    'target_endian="little"',
    'target_family="unix"',
    'target_feature="neon"',
    `target_os="${os}"`,
    'target_pointer_width="64"',
    '',
  ].join('\n');
}

describe('parseRustcCfg', () => {
  it.each([
    ['i686-pc-windows-msvc', 'windows', 'x86'],
    ['armv7-unknown-linux-gnueabihf', 'linux', 'arm'],
    ['thumbv7neon-unknown-linux-gnueabihf', 'linux', 'arm'],
    ['riscv64gc-unknown-linux-gnu', 'linux', 'riscv64'],
    ['x86_64h-apple-darwin', 'macos', 'x86_64'],
    ['powerpc64le-unknown-linux-gnu', 'linux', 'powerpc64'],
    ['aarch64-apple-darwin', 'macos', 'aarch64'],
  ])('names %s as %s-%s', (_triple, os, arch) => {
    expect(parseRustcCfg(cfg(os, arch))).toEqual({ os, arch });
  });

  it('reads Windows line endings', () => {
    expect(parseRustcCfg(cfg('windows', 'x86').replaceAll('\n', '\r\n'))).toEqual({ os: 'windows', arch: 'x86' });
  });

  it('throws without target_os', () => {
    expect(() => parseRustcCfg('target_arch="x86_64"\ntarget_feature="sse2"\n')).toThrow(/target_os/);
  });

  it('throws without target_arch', () => {
    expect(() => parseRustcCfg('target_os="linux"\ntarget_feature="sse2"\n')).toThrow(/target_arch/);
  });
});

describe('rustPlatform', () => {
  it('asks rustc for a target it has no std for', () => {
    expect(rustPlatform({ target: 'riscv64gc-unknown-linux-gnu' })).toEqual({ os: 'linux', arch: 'riscv64' });
  });

  it('throws, naming rustc, for a target rustc does not know', () => {
    expect(() => rustPlatform({ target: 'not-a-triple' })).toThrow(/rustc/);
  });
});
