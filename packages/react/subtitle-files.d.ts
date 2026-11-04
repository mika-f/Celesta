// `import captions from './captions.srt'` gives the file's text: the Celesta
// CLI bundles `.srt` and `.vtt` files as text, for `parseSubtitles()`.
// Referenced from this package's declarations, so importing `@celesta.studio/react`
// makes such imports type-check.
declare module '*.srt' {
  const source: string;
  export default source;
}

declare module '*.vtt' {
  const source: string;
  export default source;
}
