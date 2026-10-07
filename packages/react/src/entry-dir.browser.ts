// Browser assets are resolved by the embedding app, without Node's filesystem.
export function isRemoteUrl(src: string): boolean { return /^https?:\/\/./i.test(src); }
export function entryRelativePath(src: string): string { return src; }
