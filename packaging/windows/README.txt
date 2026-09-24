Celesta for Windows x64

Launch Celesta.exe. Keep the runtime directory and DLLs beside the executable.
Node.js, npm, pnpm, and FFmpeg do not need to be installed separately.

To try React, open a terminal in this directory and run:
  .\Celesta.exe .\examples\title.tsx

To export the example:
  .\Celesta-export.exe --react .\examples\title.tsx output.mp4

Choose File > Open... in Celesta to preview a project or React composition.
Keep your projects and media in your own documents directory. The examples
are starting points; copy them before editing them in a text editor.

This is an unsigned development package. Windows signing and public release
license/source-distribution review are separate release steps. Native library
licenses are in licenses/native, Rust notices in licenses/rust, Node.js notices
in runtime/LICENSE, and JavaScript notices in runtime/react/node_modules.
The x264-enabled FFmpeg build includes GPL-licensed software.
