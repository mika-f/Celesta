import { usePathname } from '@natsuneko-laboratory/makit-runtime/navigation';
import { useEffect, useRef, useState, type MouseEvent } from 'react';

/**
 * State for a button that shows and hides a panel (the header menu, the docs drawer). The panel
 * closes on Escape (focus returns to the button), on a pointer press outside both, on a link
 * inside the panel (`closeOnLink` as its `onClick`), and when the pathname changes.
 */
export function useDisclosure() {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => setOpen(false), [pathname]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setOpen(false);
      button.current?.focus();
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [open]);

  return {
    open,
    toggle: () => setOpen((value) => !value),
    button,
    panel,
    closeOnLink: (event: MouseEvent) => {
      if ((event.target as Element).closest('a')) setOpen(false);
    },
  };
}
