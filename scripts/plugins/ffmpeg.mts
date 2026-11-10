// Native FFmpeg builds must be selected per process, never via Cargo features:
// features unify within a workspace, but the plugin builds have different links.
export function ffmpegEnvironment(cargoPackage: string, environment: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const result = { ...environment };
  const variant = ({
    'celesta-codec-ffmpeg-x264': 'X264',
    'celesta-codec-ffmpeg-opus': 'OPUS',
    'celesta-codec-ffmpeg-vpx': 'VPX',
  } as Record<string, string>)[cargoPackage];
  if (variant) {
    for (const key of ['PKG_CONFIG_PATH', 'VCPKG_ROOT', 'VCPKGRS_TRIPLET']) {
      const value = environment[`FFMPEG_${variant}_${key}`];
      if (value !== undefined) result[key] = value;
    }
  }
  if ((cargoPackage === 'celesta-codec-ffmpeg' || variant !== undefined) && result.VCPKG_ROOT) {
    // vcpkg-rs does not emit rerun-if-env-changed for VCPKG_ROOT, unlike
    // pkg-config. Include the selected native search path in rustc's flags
    // so Cargo also keys ffmpeg-sys-next's artifacts by this installation.
    // Encoded flags preserve paths containing spaces and take precedence
    // over RUSTFLAGS exactly as Cargo does.
    const flags = environment.CARGO_ENCODED_RUSTFLAGS !== undefined
      ? environment.CARGO_ENCODED_RUSTFLAGS.split('\x1f').filter(Boolean)
      : (environment.RUSTFLAGS ?? '').split(/\s+/).filter(Boolean);
    const triplet = result.VCPKGRS_TRIPLET ?? 'x64-windows-static-md';
    flags.push('-L', `native=${result.VCPKG_ROOT}/installed/${triplet}/lib`);
    result.CARGO_ENCODED_RUSTFLAGS = flags.join('\x1f');
  }
  return result;
}
