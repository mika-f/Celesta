/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Download page or file for every platform. Defaults to the latest GitHub release. */
  readonly VITE_DOWNLOAD_URL?: string;
  /** macOS download. Defaults to `VITE_DOWNLOAD_URL`. */
  readonly VITE_DOWNLOAD_URL_MACOS?: string;
  /** Windows download. Defaults to `VITE_DOWNLOAD_URL`. */
  readonly VITE_DOWNLOAD_URL_WINDOWS?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
