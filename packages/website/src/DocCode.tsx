import { t, text } from './i18n';
import { useEffect, useMemo, useState } from 'react';
import { highlight } from './syntax';

export function DocCode({ code: source, label, language = 'shell' }: { code: string; label: string; language?: string }) {
  // Raw example files have CRLF line endings in a Windows checkout, which would render as blank lines.
  const code = useMemo(() => source.replace(/\r\n?/g, '\n'), [source]);
  const [status, setStatus] = useState('');
  const html = useMemo(() => highlight(code, language, { class_name: `twinkleplop language-${language}`, attributes: { tabindex: 0, 'aria-label': label } }), [code, label, language]);
  useEffect(() => {
    if (!status) return;
    const timeout = setTimeout(() => setStatus(''), 3000);
    return () => clearTimeout(timeout);
  }, [status]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setStatus(t('copied'));
    } catch {
      setStatus(t('copy-failed'));
    }
  }

  return <div className="doc-code">
    <div className="doc-code-bar">{text('code.copy', [<span />, label, <button onClick={copy} aria-label={t('copy-label', { label })} />])}</div>
    <div dangerouslySetInnerHTML={{ __html: html }} />
    <span className="doc-copy-status" role="status">{status}</span>
  </div>;
}
