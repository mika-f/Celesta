'use client';

import { RouteLink } from '@natsuneko-laboratory/makit-runtime';
import { ArrowUpRight, Download, Menu, X } from 'lucide-react';
import { usePathname } from '@natsuneko-laboratory/makit-runtime/navigation';
import { useId } from 'react';
import { useDisclosure } from './useDisclosure.ts';

export interface NavLink {
  href: string;
  label: string;
  external?: boolean;
  /** Marked current on every page below `href`. */
  section?: boolean;
}

export interface HeaderMenuProps {
  links: readonly NavLink[];
  download: { href: string; label: string };
  labels: { menu: string; close: string; main: string };
}

const trim = (path: string) => path.replace(/\/$/, '');

/** `page` on the section's own page, `true` below it. */
const currentOf = (pathname: string, href: string) => {
  const [path, base] = [trim(pathname), trim(href)];
  return path === base ? 'page' : path.startsWith(`${base}/`) ? 'true' : undefined;
};

/**
 * The header navigation. From 860 px it sits inline; below, it folds behind a disclosure button
 * (closing rules in `useDisclosure`).
 */
export function HeaderMenu({ links, download, labels }: HeaderMenuProps) {
  const id = useId();
  const pathname = usePathname();
  const { open, toggle, button, panel, closeOnLink } = useDisclosure();

  return <>
    <button ref={button} type="button" className="nr-menu-button" aria-expanded={open} aria-controls={id} onClick={toggle}>
      {open ? <X size={16} strokeWidth={1.75} aria-hidden="true" /> : <Menu size={16} strokeWidth={1.75} aria-hidden="true" />}<span className="nr-menu-label">{open ? labels.close : labels.menu}</span>
    </button>
    <div ref={panel} id={id} className="nr-nav-panel" data-open={open ? '' : undefined} onClick={closeOnLink}>
      <nav className="nr-nav" aria-label={labels.main}>
        {links.map((link) => link.external
          ? <a key={link.href} href={link.href}>{link.label}<ArrowUpRight size={14} aria-hidden="true" /></a>
          : <RouteLink key={link.href} href={link.href} aria-current={link.section ? currentOf(pathname, link.href) : undefined}>{link.label}</RouteLink>)}
      </nav>
      <a className="nr-button nr-nav-download" href={download.href}><Download size={16} strokeWidth={1.75} aria-hidden="true" />{download.label}</a>
    </div>
  </>;
}
