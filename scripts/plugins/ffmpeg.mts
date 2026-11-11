// The FFmpeg plugins' Cargo packages, in one place for every script.
/** The standard, LGPL program: decoding, muxing, and every encoder but libx264. */
export const FFMPEG_PACKAGE = 'celesta-codec-ffmpeg';
/** The optional, GPL program, which serves libx264 alone. */
export const FFMPEG_X264_PACKAGE = 'celesta-codec-ffmpeg-x264';
/** Every Cargo package that links FFmpeg, standard first. */
export const FFMPEG_PACKAGES = [FFMPEG_PACKAGE, FFMPEG_X264_PACKAGE] as const;

export function isFfmpegPackage(cargoPackage: string): boolean {
  return (FFMPEG_PACKAGES as readonly string[]).includes(cargoPackage);
}

// Native FFmpeg builds must be selected per process, never via Cargo features:
// features unify within a workspace, but the two builds have different links.
// The x264 package reads FFMPEG_X264_<variable> in place of <variable>; the
// standard package uses the environment as it is.
export function ffmpegEnvironment(cargoPackage: string, environment: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const result = { ...environment };
  if (cargoPackage === FFMPEG_X264_PACKAGE) {
    for (const key of ['PKG_CONFIG_PATH', 'VCPKG_ROOT', 'VCPKGRS_TRIPLET']) {
      const value = environment[`FFMPEG_X264_${key}`];
      if (value !== undefined) result[key] = value;
    }
  }
  if (isFfmpegPackage(cargoPackage) && result.VCPKG_ROOT) {
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
