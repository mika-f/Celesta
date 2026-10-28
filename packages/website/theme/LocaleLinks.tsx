export interface LocaleLink {
  locale: string;
  label: string;
  href: string;
  current: boolean;
}

/**
 * `EN / JA` — the language switch for every shell (Makit's `LocaleSwitcher` is not exported).
 * Plain `<a>`, not `RouteLink`: a full load lets `LocaleMemory` remember the choice.
 */
export function LocaleLinks({ links, label }: { links: readonly LocaleLink[]; label: string }) {
  if (links.length < 2) return null;
  return <nav className="nr-locale nr-mono" aria-label={label}>
    {links.map((link, index) => <span key={link.locale}>
      {index > 0 && <span className="nr-locale-sep" aria-hidden="true">/</span>}
      <a href={link.href} hrefLang={link.locale} lang={link.locale} title={link.label} aria-label={`${link.label} (${link.locale.toUpperCase()})`} aria-current={link.current ? 'true' : undefined}>{link.locale.toUpperCase()}</a>
    </span>)}
  </nav>;
}
