import { useEffect, useState } from 'react';

export function DocCode({ code, label, language = 'shell' }: { code: string; label: string; language?: string }) {
  const [status, setStatus] = useState('');
  useEffect(() => {
    if (!status) return;
    const timeout = setTimeout(() => setStatus(''), 3000);
    return () => clearTimeout(timeout);
  }, [status]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setStatus('Copied!');
    } catch {
      setStatus('Copy unavailable. Select the code to copy it manually.');
    }
  }

  return <div className="doc-code">
    <div className="doc-code-bar"><span>{label}</span><button onClick={copy} aria-label={`Copy ${label}`}>Copy <span aria-hidden="true">⧉</span></button></div>
    <pre tabIndex={0} aria-label={label}><code className={`language-${language}`}>{code}</code></pre>
    <span className="doc-copy-status" role="status">{status}</span>
  </div>;
}
