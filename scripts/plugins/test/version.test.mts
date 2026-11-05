import { describe, expect, it } from 'vitest';

import { bumpCargoToml, bumpPluginJson, workspaceVersion } from '../version.mjs';

const rootToml = `[workspace]
resolver = "3"
members = [
  "crates/foo",
]

[workspace.package]
version = "0.9.0"
edition = "2024"
rust-version = "1.89"

[workspace.dependencies]
foo = { path = "x", version = "=0.9.0" }
baz = { path = "y", version = "=0.9.0" }
serde = { version = "1.0" }
bar = { version = "0.9.0" }

[profile.dev.package.signalsmith-stretch]
opt-level = 3
`;

const codecToml = `[package]
name = "celesta-codec-ffmpeg"
version = "0.9.0"
rust-version = "1.89"

[dependencies]
celesta-composition = { path = "../../crates/composition", version = "=0.9.0" }
ez-ffmpeg = "0.19"

[target.'cfg(not(target_os = "macos"))'.dependencies]
ez-ffmpeg = { version = "0.19", features = ["static"] }

[dev-dependencies]
celesta-exporter = { path = "../../crates/exporter", version = "=0.9.0" }
tempfile = "3.23.0"
`;

describe('workspaceVersion', () => {
  it("reads [workspace.package]'s version", () => {
    expect(workspaceVersion(rootToml)).toBe('0.9.0');
  });

  it('refuses a manifest without [workspace.package]', () => {
    expect(() => workspaceVersion('[package]\nversion = "0.9.0"\n')).toThrow('no [workspace.package] version');
  });
});

describe('bumpCargoToml', () => {
  it("bumps the workspace's version and its exact pins, and nothing else", () => {
    expect(bumpCargoToml(rootToml, '0.9.0', '0.10.0', 'Cargo.toml')).toBe(
      rootToml
        .replace('version = "0.9.0"\nedition', 'version = "0.10.0"\nedition')
        .replace('version = "=0.9.0" }\nbaz', 'version = "=0.10.0" }\nbaz')
        .replace('"y", version = "=0.9.0"', '"y", version = "=0.10.0"'),
    );
  });

  it("bumps a package's version and its pins in [dependencies] and [dev-dependencies]", () => {
    expect(bumpCargoToml(codecToml, '0.9.0', '0.10.0', 'plugins/ffmpeg/Cargo.toml')).toBe(
      codecToml
        .replace('version = "0.9.0"\nrust-version', 'version = "0.10.0"\nrust-version')
        .replace('composition", version = "=0.9.0"', 'composition", version = "=0.10.0"')
        .replace('exporter", version = "=0.9.0"', 'exporter", version = "=0.10.0"'),
    );
  });

  it("refuses a package that does not give Celesta's version", () => {
    const stale = codecToml.replace('version = "0.9.0"', 'version = "0.8.0"');
    expect(() => bumpCargoToml(stale, '0.9.0', '0.10.0', 'plugins/ffmpeg/Cargo.toml')).toThrow(
      "plugins/ffmpeg/Cargo.toml: version 0.8.0 is not Celesta's 0.9.0",
    );
  });

  it('refuses a manifest with nothing to bump', () => {
    expect(() => bumpCargoToml('[dependencies]\nserde = "1.0"\n', '0.9.0', '0.10.0', 'x/Cargo.toml')).toThrow(
      'x/Cargo.toml: nothing to bump',
    );
  });
});

describe('bumpPluginJson', () => {
  const pluginJson = '{\n  "id": "ffmpeg",\n  "version": "0.9.0",\n  "roles": ["codec"]\n}\n';

  it('bumps the version and leaves every other byte', () => {
    expect(bumpPluginJson(pluginJson, '0.9.0', '0.10.0', 'plugins/ffmpeg/plugin.json')).toBe(
      '{\n  "id": "ffmpeg",\n  "version": "0.10.0",\n  "roles": ["codec"]\n}\n',
    );
  });

  it("refuses a plugin that does not give Celesta's version", () => {
    const stale = pluginJson.replace('0.9.0', '0.8.0');
    expect(() => bumpPluginJson(stale, '0.9.0', '0.10.0', 'plugins/ffmpeg/plugin.json')).toThrow(
      "plugins/ffmpeg/plugin.json: version 0.8.0 is not Celesta's 0.9.0",
    );
  });
});
