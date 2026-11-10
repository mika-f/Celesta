// Native FFmpeg builds must be selected per process, never via Cargo features:
// features unify within a workspace, but the two plugins have different links.
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
  return result;
}
