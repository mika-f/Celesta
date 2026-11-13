Celesta for Windows x64

Launch Celesta.exe. Keep the runtime directory and DLLs beside the executable.
Node.js, npm, pnpm, and FFmpeg do not need to be installed separately.

To try React, open a terminal in this directory and run:
  .\Celesta.exe .\examples\title.tsx

To export the example:
  .\Celesta-export.exe --react .\examples\title.tsx output.mov

To let a coding agent (Claude Code, Cursor, ...) check and render videos,
register Celesta-mcp.exe in this directory as an MCP server.

Choose File > Open... in Celesta to preview a project or React composition.
Keep your projects and media in your own documents directory. The examples
are starting points; copy them before editing them in a text editor.

This is an unsigned development package. Windows signing is a separate release
step. Native library licenses are in licenses/native, Rust notices in
licenses/rust, Node.js notices in runtime/LICENSE, and JavaScript notices in
runtime/react/node_modules.

Celesta's own source and main executables are MIT/Apache-2.0 licensed;
licenses/LICENSE-MIT and licenses/LICENSE-APACHE contain their license texts.
The main executables link no FFmpeg. The bundled plugins\ffmpeg provides the
standard LGPL build: decoding, ProRes, VP9, AAC, Opus, PCM and muxing.
MOV/ProRes and WebM exports work without additional downloads, including the
example above.

For MP4, use the bundled mediafoundation/h264 when available, or download
ffmpeg-x264 from the same GitHub release as Celesta. Choose the windows-x64
ZIP, extract it, and copy the whole ffmpeg-x264 folder into
%APPDATA%\Celesta\plugins. Open Celesta, reload plugins and allow it.
Keep the plugin's manifest, executable, libraries and licenses together.
FFmpeg does not need to be installed separately. The download carries its
own native and Rust notices and the GPL source offer.

The Media Foundation plugin in plugins\mediafoundation
(plugins\mediafoundation\bin\windows-x86_64\celesta-codec-mediafoundation.exe)
adds Windows' own H.264, HEVC, and AAC encoders and a decoder. It links only
Windows system libraries and is MIT/Apache-2.0 licensed
(plugins\mediafoundation\licenses).
