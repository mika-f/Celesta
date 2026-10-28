'use client';

import { useEffect } from 'react';

/**
 * Sets `document.title` to the entry for `data-nr-locale`. For pages rendered once for every
 * locale (the 404): after hydration React restores the hoisted `<title>` and Next inserts the root
 * layout's metadata title ahead of it, so this re-applies the title whenever `<head>` changes.
 */
export function DocumentTitle({ titles }: { titles: Readonly<Record<string, string>> }) {
  useEffect(() => {
    const title = titles[document.documentElement.getAttribute('data-nr-locale') ?? ''];
    if (!title) return;
    const apply = () => { if (document.title !== title) document.title = title; };
    apply();
    const observer = new MutationObserver(apply);
    observer.observe(document.head, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [titles]);
  return null;
}
