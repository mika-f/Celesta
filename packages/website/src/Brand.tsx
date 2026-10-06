import { homePath, t } from './i18n';
/** The Celesta mark: a crescent that reads as a "C", with a single star in its opening. */
export function Logo() {
  return <svg viewBox="0 0 32 32" aria-hidden="true"><path d="M24.8 7.84A12 12 0 1 0 24.8 24.16 9 9 0 1 1 24.8 7.84Z" fill="currentColor" /><circle className="brand-dot" cx="24.5" cy="16" r="2.25" /></svg>;
}

export function Brand({ href = `${homePath}#top` }: { href?: string }) {
  return <a className="brand" href={href} aria-label={t('home-label')}><Logo />Celesta</a>;
}
