export const repository = 'https://github.com/mika-f/Frameweave';

// Download links are injected at build time (see README → Download links).
const env = import.meta.env;
export const allDownloads = env.VITE_DOWNLOAD_URL || `${repository}/releases/latest`;

export type Platform = 'macOS' | 'Windows';
export const downloads: Record<Platform, string> = {
  macOS: env.VITE_DOWNLOAD_URL_MACOS || allDownloads,
  Windows: env.VITE_DOWNLOAD_URL_WINDOWS || allDownloads,
};

function detectPlatform(): Platform | null {
  const agent = navigator.userAgent;
  // iPadOS reports itself as a Mac, so check for touch as well.
  if (/iPhone|iPad|iPod|Android/.test(agent) || (/Macintosh/.test(agent) && navigator.maxTouchPoints > 1)) return null;
  if (/Mac/.test(agent)) return 'macOS';
  if (/Windows/.test(agent)) return 'Windows';
  return null;
}

/** The visitor's desktop platform, when Celesta has an app for it. */
export const visitorPlatform = detectPlatform();
