import { useEffect, useState } from 'react';
import { currentLocale, t } from './i18n';
import { localeCodes, locales, localizedPath } from './locales';

export function LanguageSwitcher() {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const update = () => setHash(window.location.hash);
    window.addEventListener('hashchange', update);
    return () => window.removeEventListener('hashchange', update);
  }, []);
  return <span className="language-switcher" role="group" aria-label={t('language')}>
    {localeCodes.map(locale => <a key={locale} href={`${localizedPath(window.location.pathname, locale)}${window.location.search}${hash}`} lang={locale} hrefLang={locale} aria-current={locale === currentLocale ? 'page' : undefined}>{locales[locale]}</a>)}
  </span>;
}
