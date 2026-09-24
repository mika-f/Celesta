import { allDownloads, downloads, visitorPlatform, type Platform } from './links';

/** The primary download call to action, matched to the visitor's platform. */
export function DownloadButton({ className = '' }: { className?: string }) {
  const href = visitorPlatform ? downloads[visitorPlatform] : allDownloads;
  return <a className={`button button-primary ${className}`} href={href}>{visitorPlatform ? `Download for ${visitorPlatform}` : 'Download Celesta'} <span aria-hidden="true">↓</span></a>;
}

/** Links to the other platforms and every release, below a DownloadButton. */
export function DownloadAlternatives({ className = '' }: { className?: string }) {
  const others = (['macOS', 'Windows'] as Platform[]).filter(platform => platform !== visitorPlatform);
  return <p className={`download-alternatives ${className}`}>
    {visitorPlatform ? 'Also for' : 'For'} {others.map((platform, i) => <span key={platform}>{i > 0 && ' and '}<a href={downloads[platform]}>{platform}</a></span>)}
    <span aria-hidden="true"> · </span><a href="/docs/#installation">Install guide</a>
    <span aria-hidden="true"> · </span><a href={allDownloads}>All releases <span aria-hidden="true">↗</span></a>
  </p>;
}
