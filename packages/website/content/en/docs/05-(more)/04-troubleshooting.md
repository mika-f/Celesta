::::accordion[Troubleshooting]{multiple}
:::toggle[The app cannot find its bundled runtime]{open}

On macOS, copy the complete app into Applications before launching it. For a portable Windows build, extract the entire archive and keep `runtime` and the DLLs beside `Celesta.exe`. If files are missing, extract or install the package again.

:::
:::toggle[A source build cannot find FFmpeg]

Confirm that the 8.1.x development libraries are installed, not just the executable. On macOS, set `PKG_CONFIG_PATH` in the same shell where you build the media plugin. On Windows, check `VCPKG_ROOT` and the MSVC build tools. On Linux, point `PKG_CONFIG_PATH` at the FFmpeg you built, such as `/opt/ffmpeg8-standard/lib/pkgconfig`. See [source-build setup](/en/docs/build-from-source/).

:::
:::toggle[Video and audio do not show, or an export fails with unsupported_output or no_decoder]

Celesta reads and writes video and audio through its media plugin, and the Problems tab says **No media decoder can run** when it has none. In a package, check that the `ffmpeg` plugin is still in the plugin folder that comes with Celesta (`plugins\ffmpeg` beside `Celesta.exe`, or `Contents/Resources/PlugIns/ffmpeg` in the macOS app), or install the package again. In a source build, run `node scripts/plugins/build.mts plugins/ffmpeg` from the repository root, and set `CELESTA_PLUGIN_PATH` to the absolute path of `plugins/dist` (`export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"`) in the shell where you run Celesta. A `plugins/dist/ffmpeg` built from an older checkout may have `"roles": ["codec"]` in its `celesta-plugin.json`, which Celesta ignores: build it again. A video export with no media plugin fails with `unsupported_output`, since no muxer writes its extension, and its hint says how to install the plugin; a still export fails with `no_decoder` when it reads a video or audio file. When the plugin's FFmpeg build lacks an encoder the video needs, or an optional encoder plugin is missing (H.264 needs `ffmpeg-x264` unless a bundled platform encoder is available; WebM needs `ffmpeg-vpx` and `ffmpeg-opus`), the export fails with `component_unavailable` and the reason: run `Celesta-export --list-components --check` to see which components work. If a platform H.264 encoder is present but unavailable, select `ffmpeg-x264/libx264` after installing that download, or choose `.mov` for ProRes with the standard plugin. The Export dialog includes these recovery hints. See [Media plugins](/en/docs/plugins/#media-plugins).

An export that fails with `unsupported_media` found decoders, but none of them reads that file: convert it to a format the `ffmpeg` plugin reads, such as MP4 or WAV, or install a plugin whose decoder reads it.

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

If `.celesta/` is in a folder above the one containing the composition, such as the project folder for a composition in `src/` or the repository root in a monorepo, that folder needs a `celesta.config.json` to be the project root. Choose **File → Set Up TypeScript in Folder…** and pick it; Celesta adds an empty `celesta.config.json` if the folder has none, and refreshes `.celesta/`.

:::
:::toggle[A composition fails because something wrote to the runtime’s stdout]

Celesta talks to a React composition’s runtime over its standard output, so a direct `process.stdout.write()` in the composition or in a package it imports breaks that connection, and loading stops with an error that says something other than the Celesta runtime wrote to its stdout. Log with `console.log()` or `console.error()` instead, which go to standard error. If a package writes to standard output on its own, check its options for a quiet or logging setting.

:::
:::toggle[An export fails with unknown_preset or invalid_presets]

`unknown_preset` means that no presets file has the name `--preset` (or MCP's and TypeScript's `preset`) gives: Celesta looks in the `--preset-file`, then the project's `export-presets.json` at the project root, then yours beside `settings.json`, and names are case-sensitive. When the message says `the default preset … is in no presets file`, the project file's `"default"` names a preset no file has: fix the name, or remove `"default"`. A name such as `veryfast` is x264's preset, which is a setting: use `--video-option preset=veryfast`, as the hint says.

`invalid_presets` means that a presets file the export had to look in cannot be read, is not valid JSON, or has a `version` other than `1`; the message names the file. Fix or remove it. When the project's preset is found first and only your file is broken, the export runs instead, with a warning that your file could not be checked for a preset of the same name. A `--preset-file` with the same problem is rejected with the other arguments instead. See [Export presets](/en/docs/export/#export-presets).

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
