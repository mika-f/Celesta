export function Brand({ href = '/#top' }: { href?: string }) {
  return <a className="brand flex items-center gap-2.5" href={href} aria-label="Celesta home"><img src="/favicon.svg" width="35" height="35" alt="" />celesta<span className="brand-period">.</span></a>;
}
