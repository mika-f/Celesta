::::accordion[Troubleshooting]{multiple}
:::toggle[The app cannot find its bundled runtime]{open}

On macOS, copy the complete app into Applications before launching it. For a portable Windows build, extract the entire archive and keep `runtime` and the DLLs beside `Celesta.exe`. If files are missing, extract or install the package again.

:::
:::toggle[A source build cannot find FFmpeg]

Confirm that the 8.1.x development libraries are installed, not just the executable. On macOS, set `PKG_CONFIG_PATH` in the same shell where you run Cargo. On Windows, check `VCPKG_ROOT` and the MSVC build tools. On Linux, point `PKG_CONFIG_PATH` at the FFmpeg you built, such as `/opt/ffmpeg8/lib/pkgconfig`. See [source-build setup](/en/docs/build-from-source/).

:::
:::toggle[Video and audio do not show, or an export fails with no_codec]

Celesta reads and writes media through its codec plugin. In a package, check that the `ffmpeg` plugin is still in the plugin folder that comes with Celesta (`plugins\ffmpeg` beside `Celesta.exe`, or `Contents/PlugIns/ffmpeg` in the macOS app), or install the package again. In a source build, run `cargo build -p celesta-codec-ffmpeg --release` so the program is beside the app’s. See [The codec plugin](/en/docs/plugins/#the-codec-plugin).

:::
:::toggle[My media is missing]

Check local paths relative to the composition entry and keep the referenced media when sharing the project. HTTP(S) media needs network access on first use; native rendering caches files by URL without checking for updates. For changed remote content, use a new URL or clear its cache. Browser media additionally requires CORS access.

:::
:::toggle[The preview did not change after saving]

React compositions reload automatically. Use **File → Reload** to retry, and check that you saved the file actually open in Celesta. The app does not edit source files.

:::
:::toggle[My text uses the wrong font]

`fontFamily` must match the family name of a font installed on the computer that renders the video, or of a font file loaded with `<Font>`. When it does not, Celesta falls back to another font and warns: the app lists the family with the preview’s warnings, and `celesta-exporter` prints a warning that names the family, weight, and text layer. Check the exact family name in your system’s font manager. To avoid installing the font on every machine that exports the project, keep the font file next to the entry and load it with `<Font>`.

:::
:::toggle[The mouth does not move]

Lip sync needs an uncompressed WAV voice and a transcript with readable vowels (kana or romaji). Check that `loadLipSync()` runs in `prepare()`, that the track is passed as `lipSync`, and that each mouth path matches a layer in the PSD exactly. For a PSD that shows nothing, set `layers` from a PSDTool preset.

:::
:::toggle[My editor cannot resolve @celesta.studio/react or another Celesta package]

Open the composition in Celesta and choose **File → Set Up TypeScript**. Check that your `tsconfig.json` extends `./.celesta/tsconfig.json`. When building from source, also complete the runtime’s install, codegen, and build steps.

:::
:::toggle[MP4 export will not start]

Use non-zero, even-numbered dimensions and confirm that all local media is available. If the destination file already exists, choose another path or explicitly add `--overwrite` to the CLI command.

:::
:::toggle[The selected graphics API is unavailable]

Try --driver auto, or choose an API supported by your platform and GPU. DX12 is Windows-only and Metal is macOS-only. For headless Linux, install Mesa lavapipe and use --driver vulkan. Check the error for a missing build backend or an unavailable adapter.

:::
::::

## Still stuck?

Open an issue with your operating system, reproduction steps, and the exact error message. Include a small project and media you have permission to share when possible.

[Report a problem on GitHub ↗](https://github.com/mika-f/Celesta/issues)
