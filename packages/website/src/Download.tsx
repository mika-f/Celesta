import { docPath } from './docs-nav';
import { t, text } from './i18n';
import { allDownloads, downloads, visitorPlatform, type Platform } from './links';

/** The primary download call to action, matched to the visitor's platform. */
export function DownloadButton({ className = '' }: { className?: string }) {
  const href = visitorPlatform ? downloads[visitorPlatform] : allDownloads;
  return <a className={`button button-primary ${className}`} href={href}>{visitorPlatform ? t('download-platform', { platform: visitorPlatform }) : t('download')} <span aria-hidden="true">↓</span></a>;
}

/** Links to the other platforms and every release, below a DownloadButton. */
export function DownloadAlternatives({ className = '' }: { className?: string }) {
  const others = (['macOS', 'Windows'] as Platform[]).filter(platform => platform !== visitorPlatform);
  return <p className={`download-alternatives ${className}`}>{text('download.install-guide-all-releases', [visitorPlatform ? t('also-for') : t('for'), others.map((platform, i) => <span key={platform}>{i > 0 && t('and')}<a href={downloads[platform]}>{platform}</a></span>), <span aria-hidden="true" />, <a href={docPath('overview') + '#installation'} />, <span aria-hidden="true" />, <a href={allDownloads} />, <span aria-hidden="true" />])}</p>;
}
