// `import source from './effect.wgsl'` gives the file's text: the Celesta
// CLI bundles `.wgsl` files as text. Referenced from this package's
// declarations, so importing `@celesta/shader` makes such imports type-check.
declare module '*.wgsl' {
  const source: string;
  export default source;
}
