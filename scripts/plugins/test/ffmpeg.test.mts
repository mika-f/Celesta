import { describe, expect, it } from 'vitest';
import { ffmpegEnvironment } from '../ffmpeg.mts';

describe('native FFmpeg selection', () => {
  const environment = {
    PKG_CONFIG_PATH: '/lgpl/lib/pkgconfig', VCPKG_ROOT: '/lgpl', VCPKGRS_TRIPLET: 'base',
    FFMPEG_X264_PKG_CONFIG_PATH: '/gpl/lib/pkgconfig', FFMPEG_X264_VCPKG_ROOT: '/gpl',
    FFMPEG_X264_VCPKGRS_TRIPLET: 'gpl', PATH: '/bin',
  };
  it('keeps the default and unrelated packages on LGPL libraries', () => {
    for (const name of ['celesta-codec-ffmpeg', 'celesta-plugin-vrm']) {
      expect(ffmpegEnvironment(name, environment)).toEqual(environment);
    }
  });
  it.each(['OPUS', 'VPX'])('selects the %s prefix independently', (variant) => {
    const selected = ffmpegEnvironment(`celesta-codec-ffmpeg-${variant.toLowerCase()}`, {
      ...environment, [`FFMPEG_${variant}_PKG_CONFIG_PATH`]: `/${variant}/lib/pkgconfig`,
      [`FFMPEG_${variant}_VCPKG_ROOT`]: `/${variant}`,
    });
    expect(selected.PKG_CONFIG_PATH).toBe(`/${variant}/lib/pkgconfig`);
    expect(selected.VCPKG_ROOT).toBe(`/${variant}`);
  });
  it('switches only the GPL process, without mutating the caller', () => {
    expect(ffmpegEnvironment('celesta-codec-ffmpeg-x264', environment)).toMatchObject({
      PKG_CONFIG_PATH: '/gpl/lib/pkgconfig', VCPKG_ROOT: '/gpl', VCPKGRS_TRIPLET: 'gpl', PATH: '/bin',
    });
    expect(environment.PKG_CONFIG_PATH).toBe('/lgpl/lib/pkgconfig');
  });
});
