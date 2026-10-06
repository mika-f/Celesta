import { currentLocale, homePath, t } from './i18n';
import { localeCodes, locales } from './locales';

export function LanguageSwitcher() {
  const path = window.location.pathname.startsWith(homePath)
    ? window.location.pathname.slice(homePath.length)
    : window.location.pathname.replace(/^\//, '');
  return <span className="language-switcher" role="group" aria-label={t('language')}>
    {localeCodes.map(locale => <a key={locale} href={`/${locale}/${path}${window.location.search}${window.location.hash}`} lang={locale} hrefLang={locale} aria-current={locale === currentLocale ? 'page' : undefined}>{locales[locale]}</a>)}
  </span>;
}
