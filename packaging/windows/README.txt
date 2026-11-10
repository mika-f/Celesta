Celesta for Windows x64

Launch Celesta.exe. Keep the runtime directory and DLLs beside the executable.
Node.js, npm, pnpm, and FFmpeg do not need to be installed separately.

To try React, open a terminal in this directory and run:
  .\Celesta.exe .\examples\title.tsx

To export the example:
  .\Celesta-export.exe --react .\examples\title.tsx output.mp4

To let a coding agent (Claude Code, Cursor, ...) check and render videos,
register Celesta-mcp.exe in this directory as an MCP server.

Choose File > Open... in Celesta to preview a project or React composition.
Keep your projects and media in your own documents directory. The examples
are starting points; copy them before editing them in a text editor.

This is an unsigned development package. Windows signing is a separate release
step. Native library licenses are in licenses/native, Rust notices in
licenses/rust, Node.js notices in runtime/LICENSE, and JavaScript notices in
runtime/react/node_modules.

Celesta.exe, Celesta-export.exe, and Celesta-mcp.exe link no FFmpeg and are
MIT/Apache-2.0 licensed. Video and audio are read and written by the media
plugins in plugins\ffmpeg and plugins\mediafoundation, separate programs.
The codec plugin in plugins\ffmpeg
(plugins\ffmpeg\bin\windows-x86_64\celesta-codec-ffmpeg.exe, with its own MSVC
runtime DLLs beside it) is built with an x264-enabled FFmpeg, which is
GPL-2.0-or-later licensed. See plugins\ffmpeg\licenses (or
licenses/LICENSE-GPL-2.0) for the license text and
licenses/GPL-SOURCE-OFFER.md for the corresponding-source offer.
The build also includes libvpx and libopus (BSD-3-Clause); their notices are in
licenses/native.
The Media Foundation plugin in plugins\mediafoundation
(plugins\mediafoundation\bin\windows-x86_64\celesta-codec-mediafoundation.exe)
adds Windows' own H.264, HEVC, and AAC encoders and a decoder. It links only
Windows system libraries and is MIT/Apache-2.0 licensed
(plugins\mediafoundation\licenses).
