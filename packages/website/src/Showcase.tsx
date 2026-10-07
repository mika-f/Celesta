import { useEffect, useState } from 'react';
import { Brand } from './Brand';
import { Playground } from './Playground';
import { repository } from './links';
import { showcase, showcasePath } from './showcase-catalog';
import { loadShowcase, posters } from './showcase-sources';
import { currentLocale, homePath, t } from './i18n';
import { docPath } from './docs-nav';
import { LanguageSwitcher } from './LanguageSwitcher';

export function Showcase() {
  const [menuOpen, setMenuOpen] = useState(false);
  const slug = window.location.pathname.replace(/^\/(?:[a-z]+\/)?showcase\/?/, '').replace(/\/+$/, '');
  const path = (slug = '') => showcasePath(slug, currentLocale);
  const sample = showcase.find(item => item.slug === slug);
  const [files, setFiles] = useState<Record<string, string> | null>(null);
  const [error, setError] = useState('');
  const [loadingAttempt, setLoadingAttempt] = useState(0);

  useEffect(() => {
    if (!sample || 'desktop' in sample) return;
    let current = true;
    setError('');
    void loadShowcase(sample.slug).then(result => { if (current) setFiles(result); }).catch(cause => { if (current) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { current = false; };
  }, [sample, loadingAttempt]);

  return <>
    <a href="#main" className="skip-link">{t('home.skip-to-content')}</a>
    <header className="header"><div className="shell header-inner flex items-center justify-between"><Brand /><button className="menu-toggle" aria-expanded={menuOpen} aria-controls="showcase-navigation" onClick={() => setMenuOpen(!menuOpen)}>{t(menuOpen ? 'common.navigation.close' : 'common.navigation.menu')}</button><nav id="showcase-navigation" className={`nav flex items-center ${menuOpen ? 'is-open' : ''}`} aria-label={t('home.main-navigation')} onClick={() => setMenuOpen(false)}><a href={path()} aria-current={!sample ? 'page' : undefined}>{t('showcase.label')}</a><a href={docPath('overview')}>{t('showcase.docs')}</a><LanguageSwitcher /></nav></div></header>
    <main id="main">
      <section className="showcase-intro shell" aria-labelledby="showcase-title">
        <p className="eyebrow">{sample ? <a href={path()}>{t('showcase.all')}</a> : t('showcase.made-with')}</p>
        <h1 id="showcase-title">{sample?.title ?? t('showcase.heading')}</h1>
        <p>{sample ? t(`showcase.samples.${sample.slug}.description`) : t('showcase.intro')}</p>
        {sample && <div className="showcase-meta"><span>{sample.duration}</span><a href={`${repository}/tree/main/examples/${sample.slug}`}>{t('showcase.credits')}</a></div>}
      </section>
      {sample ? 'desktop' in sample ? <section className="showcase-notice shell"><h2>{t('showcase.desktop-heading')}</h2><p>{t(`showcase.samples.${sample.slug}.limitation`)}</p><div className="flex flex-wrap gap-3"><a className="button button-primary" href={`${repository}/tree/main/examples/${sample.slug}`}>{t('showcase.setup')}</a><a className="button button-secondary" href={docPath('installation')}>{t('showcase.install')}</a></div></section>
        : files ? <Playground key={sample.slug} initialFiles={files} entry="film.tsx" initialFrame={sample.frame} baseURL={new URL(`/showcase-assets/examples/${sample.slug}/`, window.location.origin).href} silent title={t('showcase.edit-title')} />
          : <div className="shell showcase-loading" role="status">{error ? <>{error} <button className="copy-button" onClick={() => setLoadingAttempt(attempt => attempt + 1)}>{t('showcase.retry')}</button></> : t('showcase.loading')}</div>
        : <section className="shell showcase-gallery" aria-label={t('showcase.compositions')}>
          <p className="showcase-note">{t('showcase.note')}</p>
          <div className="showcase-grid">{showcase.map((item, index) => {
            const poster = posters[`../../../examples/${item.slug}/poster.jpg`];
            return <a className="showcase-card" key={item.slug} href={path(item.slug)}>
              <div className="showcase-poster">{poster ? <img src={poster} alt="" loading="lazy" width="1920" height="1080" /> : <span aria-hidden="true">{item.title.toUpperCase()}</span>}</div>
              <div className="showcase-card-meta"><span>{String(index + 1).padStart(2, '0')} · {item.duration}</span><span>{'desktop' in item ? t('showcase.desktop') : t('showcase.run')}</span></div>
              <h2>{item.title}</h2><p>{t(`showcase.samples.${item.slug}.description`)}</p>
            </a>;
          })}</div>
        </section>}
      {sample && !('desktop' in sample) && <p className="shell showcase-note">{t('showcase.edition-note')} <a href={`${repository}/tree/main/examples/${sample.slug}`}>{t('showcase.original')}</a></p>}
    </main>
    <footer className="footer shell"><div className="footer-bottom flex flex-wrap justify-between gap-3"><a href={path()}>{t('showcase.explore')}</a><a href={homePath}>Celesta</a><span>MIT / Apache-2.0</span></div></footer>
  </>;
}
