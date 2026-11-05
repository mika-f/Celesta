// The platform a plugin's executable is for, as manifests name it: `<os>-<arch>`
// in the values of Rust's `std::env::consts::{OS, ARCH}`, which the host
// matches a manifest's `executable` keys against. rustc says them itself for
// any target (`target_os` and `target_arch` in `rustc --print cfg`), so no
// table of triples is kept here.
import { spawnSync } from 'node:child_process';

/**
 * @typedef {object} Platform
 * @property {string} os `std::env::consts::OS` of the target, such as `macos`.
 * @property {string} arch `std::env::consts::ARCH` of the target, such as `aarch64`.
 */

/**
 * The OS and the architecture in the output of `rustc --print cfg`.
 *
 * @param {string} output
 * @returns {Platform}
 */
export function parseRustcCfg(output) {
  /** @type {Record<string, string>} */
  const values = {};
  for (const line of output.split(/\r?\n/)) {
    const match = /^target_(os|arch)="(.*)"$/.exec(line);
    if (match) values[match[1]] = match[2];
  }
  const { os, arch } = values;
  if (os === undefined) throw new Error('rustc --print cfg named no target_os');
  if (arch === undefined) throw new Error('rustc --print cfg named no target_arch');
  return { os, arch };
}

/**
 * The platform rustc builds for: the host's, or `target`'s. It runs
 * `$RUSTC` (or `rustc`) in `cwd`, so a `rust-toolchain.toml` there picks the
 * toolchain.
 *
 * @param {{ target?: string, cwd?: string }} [options]
 * @returns {Platform}
 */
export function rustPlatform({ target, cwd } = {}) {
  const rustc = process.env.RUSTC ?? 'rustc';
  const args = ['--print', 'cfg', ...(target === undefined ? [] : ['--target', target])];
  const command = `${rustc} ${args.join(' ')}`;
  const result = spawnSync(rustc, args, { cwd, encoding: 'utf8' });
  if (result.error) throw new Error(`${command} failed: ${result.error.message}`);
  if (result.status !== 0) {
    const reason = result.status === null ? `signal ${result.signal}` : `exit code ${result.status}`;
    throw new Error(`${command} failed (${reason}): ${result.stderr.trim()}`);
  }
  return parseRustcCfg(result.stdout);
}
