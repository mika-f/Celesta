import { useState, type ReactNode } from 'react';
import { DocCode } from './DocCode';
import { Api, Note } from './docs-shared';
import { allDownloads, downloads, repository } from './links';
import firstScene from './examples/first-scene.tsx?raw';
import dialogueScene from './examples/dialogue.tsx?raw';
import dialogueProject from './examples/dialogue.celesta.json?raw';
import lipSyncScene from './examples/lip-sync.tsx?raw';
import mediaScene from './examples/media.tsx?raw';
import timelineExample from '../../../examples/editor-demo.celesta.json?raw';
import withProject from '../../react/examples/with-project.tsx?raw';

export { repository };

function SourceBuild() {
  const [platform, setPlatform] = useState('macOS');
  return <>
    <p>To build Celesta, install <strong>Rust 1.89 or later</strong>, your platform’s native build tools, and <strong>FFmpeg 8.1.x development libraries</strong>. React compositions also need Node.js 18 or later and pnpm.</p>
    <Note title="The libraries matter">The FFmpeg command-line executable alone is not enough. Celesta links against the development libraries. FFmpeg 9 and later are not supported yet.</Note>
    <h3>1. Prepare your platform</h3>
    <div className="platform-options" role="group" aria-label="Source build platform">{['macOS', 'Windows', 'Linux'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    <div className="platform-guide">
      {platform === 'macOS' && <><h4>macOS</h4><p>Install the Xcode Command Line Tools and Homebrew. Then install the libraries and point <code>pkg-config</code> at the keg-only FFmpeg package in the shell where you build Celesta.</p><DocCode label="macOS dependencies" code={'xcode-select --install\nbrew install ffmpeg@8 pkg-config\nexport PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"'} /></>}
      {platform === 'Windows' && <><h4>Windows</h4><p>Install the MSVC C++ build tools and vcpkg. Set <code>VCPKG_ROOT</code> to your vcpkg checkout, then install the FFmpeg package below. Run Cargo from a shell configured for the MSVC toolchain.</p><DocCode label="Windows dependencies" code={'vcpkg install ffmpeg[x264]:x64-windows-static-md'} /><p>Your vcpkg checkout must provide the supported FFmpeg 8.1.x version.</p></>}
      {platform === 'Linux' && <><h4>Linux</h4><p>Most distributions, including Ubuntu 24.04 LTS, do not package FFmpeg 8.1.x, so build it from source with the repository’s <code>scripts/build-ffmpeg-linux.sh</code> after you get the source in step 2. It builds static libraries with <code>libx264</code> into <code>/opt/ffmpeg8</code>, which takes about 5 minutes on 4 cores. Cargo uses <code>libclang-dev</code> to generate the FFmpeg bindings. On Debian and Ubuntu, run these from the repository root:</p><DocCode label="Linux dependencies" code={'sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libclang-dev\nsudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev\nsudo scripts/build-ffmpeg-linux.sh /opt/ffmpeg8\nexport PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig'} /><p>Set <code>PKG_CONFIG_PATH</code> in every shell where you run Cargo. FFmpeg is linked into Celesta’s executables, so <code>LD_LIBRARY_PATH</code> is not needed. If your distribution does provide FFmpeg 8.1.x, install <code>pkg-config</code> and its <code>libavcodec</code>, <code>libavformat</code>, <code>libavfilter</code>, <code>libavdevice</code>, <code>libavutil</code>, <code>libswscale</code>, and <code>libswresample</code> development packages instead of running the script. To export without a GPU, see <a href="/docs/export/">Export a video</a>.</p></>}
    </div>
    <h3>2. Get the source</h3>
    <DocCode label="Clone Celesta" code={`git clone ${repository}.git celesta\ncd celesta`} />
    <h3>3. Open your first project</h3>
    <DocCode label="Launch the native app" code="cargo run -p celesta-editor --release" />
    <p>Celesta opens its built-in demo. You should see a preview, timeline, Assets panel, and Inspector. You can also pass a project path:</p>
    <DocCode label="Open the example project" code="cargo run -p celesta-editor --release -- examples/editor-demo.celesta.json" />
    <h3>4. Build the React runtime</h3><p>For React compositions in a source build, install the package dependencies, generate the shared types, and build the runtime:</p>
    <DocCode label="Build the React runtime from source" code={'pnpm install\npnpm --dir packages/react run codegen\npnpm --dir packages/react run build'} />
    <p>Run these commands from the repository root. The React examples under <code>examples/</code> are pnpm workspace packages, so this also installs their dependencies. To start your own project:</p>
    <DocCode label="Initialize a project from source" code="cargo run -p celesta-editor --release -- --init my-video" />
    <p>See <a href="/docs/create-project/">Create a project</a> for the generated files and dependency setup.</p>
    <DocCode label="Preview React from source" code="cargo run -p celesta-editor --release -- packages/react/examples/title.tsx" />
    <DocCode label="Export React from source" code="cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx output.mp4" />
    <p>Run Cargo commands from the repository root. Packaged-app users can skip this entire section and go straight to their <a href="/docs/react-compositions/">first React composition</a>.</p>
  </>;
}


function Installation() {
  const [platform, setPlatform] = useState('macOS');
  return <>
    <p>Download the Celesta package for your operating system from the release assets. Check the release notes for the supported OS versions and available architectures.</p>
    <div className="doc-downloads"><a className="button button-primary doc-download" href={downloads.macOS}>Download for macOS <span aria-hidden="true">↓</span></a><a className="button button-secondary" href={downloads.Windows}>Download for Windows <span aria-hidden="true">↓</span></a></div>
    <p className="doc-small">Looking for another version or architecture? <a href={allDownloads}>Browse all releases</a>.</p>
    <Note title="Everything you need to run Celesta">The macOS and Windows packages include the React runtime, Node.js, and the media dependencies. You do not need to install Rust, Node.js, pnpm, or FFmpeg separately to create and export videos.</Note>
    <h3>1. Install the app</h3>
    <div className="platform-options" role="group" aria-label="Installation platform">{['macOS', 'Windows', 'Linux'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    <div className="platform-guide">
      {platform === 'macOS' && <><h4>macOS</h4><ol><li>Choose the <code>macos-arm64.dmg</code> package for Apple Silicon, or <code>macos-x64.dmg</code> for an Intel Mac. The full filename includes the release version.</li><li>Open the disk image and drag <strong>Celesta</strong> into <strong>Applications</strong>.</li><li>Eject the disk image, then open Celesta from Applications.</li></ol></>}
      {platform === 'Windows' && <><h4>Windows x64</h4><p>Download the <code>windows-x64-setup.exe</code> installer, run it, and open Celesta from the Start menu. The full filename includes the release version.</p><p>Prefer a portable app? Download the <code>windows-x64.zip</code> package, extract the entire archive, and launch <code>Celesta.exe</code>. Keep the runtime directory and DLLs beside the executable.</p></>}
      {platform === 'Linux' && <><h4>Linux</h4><p>Use the <a href="/docs/build-from-source/">source-build instructions</a> for Linux. Check the release assets for any additional platform packages provided with a release.</p></>}
    </div>
    <h3>2. Create your project</h3><p>Choose <strong>File → Create New Project…</strong>, or the toolbar’s <strong>Create New Project…</strong> button, and select a folder such as <code>Documents/my-video</code>. The folder picker can create a new folder. Celesta initializes it and opens <code>film.tsx</code>. Keep projects separate from the installed app. See <a href="/docs/create-project/">Create a project</a> for the CLI equivalent and generated files.</p>
    <h3>3. Edit your first scene</h3><p>Open <code>film.tsx</code> in your text editor, change the title or colors, and save. Celesta reloads the preview. Press <kbd>Space</kbd> to play. For animation, save the <a href="/docs/react-compositions/">React title example</a> as <code>first-scene.tsx</code>, then choose <strong>File → Open…</strong> and select it.</p>
    <p>Prefer an existing example? The app package includes <code>title.tsx</code>, <code>editor-demo.celesta.json</code>, and <code>minimal.celesta.json</code>. See <a href="/docs/examples/">where to find them</a>.</p>
    <h3>Updating Celesta</h3><p>Quit Celesta and install the newer release package. For portable Windows builds, extract the new version into its own folder and launch it from there. Keep your projects and media in your Documents directory so they stay separate from app updates.</p>
    <p>Working on Celesta itself? See <a href="/docs/build-from-source/">Build from source</a> for the developer setup.</p>
  </>;
}

function CreateProject() {
  const [platform, setPlatform] = useState('macOS');
  const executable = platform === 'macOS'
    ? '"/Applications/Celesta.app/Contents/MacOS/Celesta"'
    : '& "C:\\path\\to\\Celesta\\Celesta.exe"';
  return <>
    <p>Initialize a folder for your React composition, media, and npm dependencies. Celesta includes the runtime needed to preview and export the starter scene.</p>
    <h3>From the app</h3>
    <p>Choose <strong>File → Create New Project…</strong>, click <strong>Create New Project…</strong> in the toolbar, or press <kbd>⌘ N</kbd> on macOS / <kbd>Ctrl N</kbd> on Windows and Linux. Select a project folder; the folder picker can create one. Celesta initializes that folder and opens <code>film.tsx</code>.</p>
    <h3>From the command line</h3>
    <p>With <code>celesta-editor</code> on PATH, pass the folder to <code>--init</code>. Omit the folder to initialize the current directory:</p>
    <DocCode label="Initialize and open a project" code={'celesta-editor --init my-video\ncd my-video\ncelesta-editor film.tsx\ncelesta-exporter --react film.tsx output.mp4'} />
    <DocCode label="Initialize the current directory" code="celesta-editor --init" />
    <p>Desktop packages name the executables <code>Celesta</code> and <code>Celesta-export</code> (<code>.exe</code> on Windows). Use their installed paths in place of <code>celesta-editor</code> and <code>celesta-exporter</code>, including in generated scripts when needed:</p>
    <div className="platform-options" role="group" aria-label="Project initialization platform">{['macOS', 'Windows'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    {platform === 'Windows' ? <p>Use PowerShell and replace <code>C:\path\to\Celesta</code> with your installed or extracted app folder. The installer does not add Celesta to PATH.</p> : <p>This command assumes the app is installed in <code>/Applications</code>. Adjust the path if needed.</p>}
    <DocCode label="Initialize with the installed app" code={`${executable} --init my-video`} />
    <p>For a <a href="/docs/build-from-source/">source build</a>, build the React runtime first, then run <code>cargo run -p celesta-editor --release -- --init my-video</code> from the repository root.</p>
    <h3>What initialization creates</h3>
    <ul>
      <li><code>film.tsx</code>: a five-second, 1920 × 1080 composition at 30 fps.</li>
      <li><code>package.json</code>: a private project with preview, export, and typecheck scripts, and TypeScript <code>^7.0.2</code> as a development dependency.</li>
      <li><code>tsconfig.json</code> and <code>.celesta/</code>: configuration and declarations matching Celesta’s bundled runtime.</li>
      <li><code>.gitignore</code>: ignores <code>node_modules/</code> and <code>output.mp4</code>. The generated <code>.celesta/</code> directory ignores itself.</li>
    </ul>
    <p>Preview and export work without installing packages. To use the project’s TypeScript compiler, install Node.js and pnpm separately, then run these commands in the project folder:</p>
    <DocCode label="Install the TypeScript compiler and check your project" code={'pnpm install\npnpm typecheck'} />
    <Note title="Initializing an existing folder">Re-running initialization refreshes <code>.celesta/</code> and creates missing starter files. Existing source files, <code>package.json</code> dependencies and scripts, <code>tsconfig.json</code>, and <code>.gitignore</code> are preserved. If an existing TypeScript configuration does not extend <code>./.celesta/tsconfig.json</code>, Celesta reports the change to make; merge it into your configuration.</Note>
    <h3>Add npm dependencies</h3>
    <p>Install external packages in the project that imports them. For a project outside the Celesta repository, run this in its folder:</p>
    <DocCode label="Add a project dependency" code="pnpm add ag-psd" />
    <p>Preview and export resolve external imports from the importing file’s project <code>node_modules</code>. Celesta supplies <code>@celesta/react</code>, <code>@celesta/math</code>, <code>@celesta/code</code>, and React from its runtime; you do not need to add them to an external project.</p>
    <p>Node asset preparation scripts use the same dependencies. Save this as <code>prepare-assets.mjs</code> beside <code>package.json</code>, with <code>portrait.psd</code> in the same folder:</p>
    <DocCode label="prepare-assets.mjs" language="tsx" code={"import { readFileSync, writeFileSync } from 'node:fs';\nimport { readPsd } from 'ag-psd';\n\nconst { width, height } = readPsd(\n  readFileSync(new URL('./portrait.psd', import.meta.url)),\n  { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true },\n);\nwriteFileSync(\n  new URL('./portrait-info.json', import.meta.url),\n  JSON.stringify({ width, height }),\n);"} />
    <DocCode label="Prepare assets" code="node prepare-assets.mjs" />
    <p>Import the generated JSON in your composition as usual. Direct Node scripts use your installed Node.js; the app’s bundled Node.js evaluates compositions. For <code>.ts</code> scripts, use a Node.js version with TypeScript stripping support or a TypeScript runner installed in the project.</p>
    <h3>Dependencies in repository examples</h3>
    <p>The React examples under <code>examples/</code> belong to the pnpm workspace. Run <code>pnpm install</code> from the repository root, then add dependencies to the example that needs them:</p>
    <DocCode label="Add a dependency to an example and preview it" code={'pnpm --dir examples/versus add ag-psd\ncargo run -p celesta-editor --release -- examples/versus/film.tsx\ncargo run -p celesta-exporter --release -- --react examples/versus/film.tsx output.mp4'} />
    <p>Example manifests use <code>workspace:*</code> for local Celesta packages so Node tools can resolve their imports too. Complete the <a href="/docs/build-from-source/">source-build setup</a> before running Cargo commands.</p>
    <p>Continue with <a href="/docs/react-compositions/">your first React composition</a>, <a href="/docs/preview/">preview controls</a>, or <a href="/docs/export/">MP4 export</a>.</p>
  </>;
}

function ExportCommands() {
  const [platform, setPlatform] = useState('macOS');
  const executable = platform === 'macOS'
    ? '"/Applications/Celesta.app/Contents/MacOS/Celesta-export"'
    : '& "C:\\path\\to\\Celesta\\Celesta-export.exe"';
  return <>
    <p>The app includes a command-line exporter. Open a terminal in your project folder, where <code>first-scene.tsx</code> or <code>project.celesta.json</code> is saved. Select your platform:</p>
    <div className="platform-options" role="group" aria-label="Exporter platform">{['macOS', 'Windows'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    {platform === 'Windows' ? <p>Use PowerShell. Replace <code>C:\path\to\Celesta</code> with your installed or extracted Celesta folder. The installer does not add Celesta to PATH.</p> : <p>These commands assume Celesta is installed in <code>/Applications</code>. Adjust the app path if you installed it elsewhere.</p>}
    <DocCode label="Export a React composition" code={`${executable} --react first-scene.tsx output.mp4`} />
    <DocCode label="Export a JSON project" code={`${executable} project.celesta.json output.mp4`} />
    <p>Add <code>--overwrite</code> to replace an existing output file. Use <code>--from</code> and <code>--to</code> to select a time range:</p>
    <DocCode label="Export the first second" code={`${executable} --from 0 --to 1 --react first-scene.tsx section.mp4`} />
    <p>To combine React with the <a href="/docs/timelines/">JSON timeline example</a>, save both files in your project folder and pass the companion project:</p>
    <DocCode label="Export a combined composition" code={`${executable} --react with-project.tsx --project project.celesta.json output.mp4`} />
    <p>For scripts and AI agents, add <code>--json</code>. The exporter then prints no progress, only one line of JSON when it finishes: the status, every file it wrote, warnings, the composition's size, fps and frame count once the exporter has loaded it, and on failure an error code and message. When the arguments are rejected, the JSON has only the status and the error.</p>
    <p>Using a source build? The <a href="/docs/build-from-source/">developer instructions</a> include the equivalent Cargo command.</p>
  </>;
}

export const contents: Record<string, ReactNode> = {
  'installation': <Installation />,
  'create-project': <CreateProject />,
  'preview': <>
      <p>Choose <strong>File → Open…</strong> to open a <code>.celesta.json</code> project or a React composition (<code>.tsx</code>, <code>.jsx</code>, <code>.ts</code>, or <code>.js</code>). Select an asset, track, or clip to inspect its details.</p>
      <Note title="Your source is your canvas">Edit projects and compositions in your text editor. The Celesta app previews and exports them; it does not edit the project itself.</Note>
      <p>React compositions reload when you save. For a JSON project, choose <strong>File → Reload</strong> after changing the source. Keep referenced media files available; missing assets are marked in the Assets panel.</p>
      <div className="doc-table-wrap" tabIndex={0} aria-label="Keyboard shortcuts"><table><caption>Keyboard shortcuts</caption><thead><tr><th>Action</th><th>macOS</th><th>Windows / Linux</th></tr></thead><tbody>
        <tr><td>Create a new project</td><td><kbd>⌘ N</kbd></td><td><kbd>Ctrl N</kbd></td></tr>
        <tr><td>Open a project</td><td><kbd>⌘ O</kbd></td><td><kbd>Ctrl O</kbd></td></tr>
        <tr><td>Reload a JSON project</td><td><kbd>⌘ R</kbd></td><td><kbd>Ctrl R</kbd></td></tr>
        <tr><td>Play / pause</td><td colSpan={2}><kbd>Space</kbd></td></tr>
        <tr><td>Step backward / forward</td><td colSpan={2}><kbd>←</kbd> / <kbd>→</kbd></td></tr>
        <tr><td>Mark export start / end</td><td colSpan={2}><kbd>I</kbd> / <kbd>O</kbd></td></tr>
      </tbody></table></div>
      <h3>Listen as you look</h3><p>Playback synchronizes the picture and audio. Use waveforms and track levels to check timing, mute or solo tracks to isolate a sound, and adjust the preview volume. Drag along the timeline ruler to scrub to a specific moment.</p>
      <h3>Preview-only guides</h3><p><code>{'<DebugOverlay />'}</code> draws the frame edge, a safe-area inset, and the center point, and <code>{'<DebugBounds>'}</code> outlines a group of layers. Both appear only in the Celesta preview and add nothing to an export, so you can leave them in while you work. <code>useIsPreview()</code> tells your own components the same thing.</p>
    </>,
  'react-compositions': <>
      <p>The installed app already includes the React runtime. <a href="/docs/create-project/">Create a project</a> from the app or CLI to get a starter <code>film.tsx</code> and TypeScript configuration. The starter previews and exports without a package installation or build step. Install any additional npm dependencies in your project folder.</p>
      <h3>Make your first scene</h3><p>Create <code>first-scene.tsx</code> in your own project folder. This example creates a 1920 × 1080 composition at 30 fps. The title fades in over 30 frames, then holds until the five-second composition ends.</p>
      <DocCode label="first-scene.tsx" language="tsx" code={firstScene.trim()} />
      <p>In Celesta, choose <strong>File → Open…</strong> and select <code>first-scene.tsx</code>. Press <kbd>Space</kbd> to play.</p>
      <p>Change the text or colors and save. The native preview reloads the composition. Celesta components describe video layers: use <code>Text</code>, <code>Rect</code>, and <code>Group</code> rather than HTML elements or CSS classes.</p>
      <h3>How a composition is put together</h3>
      <ul>
        <li>The file’s <strong>default export</strong> returns a single <code>{'<Composition>'}</code> with its size, frame rate, and length.</li>
        <li>Everything inside it is drawn in order: later layers appear on top of earlier ones.</li>
        <li>Your own components are ordinary React function components. Use props, <code>.map()</code>, and conditionals as you would anywhere else.</li>
        <li>An optional named export, <code>prepare()</code>, runs once before the first frame. See <a href="/docs/data/">Data, properties & components</a>.</li>
      </ul>
      <h3>Set up TypeScript in your editor</h3><p>With a React composition open, choose <strong>File → Set Up TypeScript</strong>. Celesta copies matching declarations for <code>@celesta/react</code>, <code>@celesta/math</code>, <code>@celesta/code</code>, React, and Node.js into a local <code>.celesta/</code> directory.</p>
      <p>Starting a new project with no composition yet? Choose <strong>File → Set Up TypeScript in Folder…</strong> and pick the project folder, so your first component is written with types in place.</p>
      <p>If there is no <code>tsconfig.json</code>, Celesta creates one. If you already have a configuration, add this <code>extends</code> property while preserving your other settings:</p>
      <DocCode label="tsconfig.json" language="json" code={'{\n  "extends": "./.celesta/tsconfig.json"\n}'} />
      <p>You do not need to install those declaration packages from npm for this editor setup. Celesta refreshes <code>.celesta/</code> when you open the project in a newer app version; that directory ignores itself in Git.</p>
    </>,
  'animation': <>
      <p>A composition’s duration in seconds is <code>durationInFrames / fps</code>. At 30 fps, 150 frames make five seconds, with frame indices from 0 through 149. Use <code>useCurrentFrame()</code> inside a composition to calculate animated values.</p>
      <h3>Interpolate between values</h3><p>The title example uses <code>Math.min(frame / 30, 1)</code> for opacity. For easing and more control, import <code>interpolate</code> and <code>Easings</code> from <code>@celesta/react</code>, then replace the calculation with:</p>
      <DocCode label="An eased fade (inside a component)" language="tsx" code={"const opacity = interpolate(frame, [0, 30], [0, 1], {\n  easing: Easings.easeOut,\n  extrapolateLeft: 'clamp',\n  extrapolateRight: 'clamp',\n});"} />
      <p>Pass the result to <code>{'<Text opacity={opacity} … />'}</code>. Input ranges must be strictly increasing and have the same number of entries as the output range. Extrapolation defaults to <code>extend</code>; use <code>clamp</code> to hold the first and last values.</p>
      <p><code>Easings</code> covers the familiar easings.net curves: <code>linear</code>, and the <code>easeIn</code>/<code>easeOut</code>/<code>easeInOut</code> variants of <code>Sine</code>, <code>Quad</code>, <code>Cubic</code>, <code>Quart</code>, <code>Quint</code>, <code>Expo</code>, <code>Circ</code>, <code>Back</code>, <code>Elastic</code>, and <code>Bounce</code> (for example <code>Easings.easeOutBack</code>).</p>
      <h3>Interpolate between colors</h3><p>Import <code>interpolateColor</code> from <code>@celesta/react</code> too. It takes the same input range and <code>easing</code> as <code>interpolate</code>, but maps the frame onto <code>#RRGGBB</code> or <code>#RRGGBBAA</code> colors and returns <code>#RRGGBBAA</code>. Use it wherever a color goes: a <code>Rect</code> fill, a text style’s fill, a gradient stop, or a shadow.</p>
      <DocCode label="Night to day, and a title fading in" language="tsx" code={"const sky = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);\nconst title = interpolateColor(frame, [0, 20], ['#FFFFFF00', '#FFFFFF']);\n\n<Rect width={1920} height={1080} fill={sky} />\n<Text style={{ fontSize: 96, fill: { type: 'solid', color: title } }}>Morning</Text>"} />
      <p>Unlike <code>interpolate</code>, extrapolation defaults to <code>clamp</code>, so the first and last colors hold outside the range; <code>extend</code> continues the end segments, and <code>identity</code> is not available. Colors blend in sRGB with premultiplied alpha, as gradients and CSS do: a color fading to <code>#00000000</code> keeps its hue instead of darkening, and channels that <code>extend</code> or an overshooting easing push out of range are clamped. CSS color names, <code>rgb()</code>, and the short <code>#RGB</code> form are not accepted.</p>
      <h3>Add a little bounce</h3><p><code>spring()</code> simulates a damped spring: it starts at 0, settles at 1, and may overshoot on the way. It needs the frame rate, so read it from <code>useVideoConfig()</code>:</p>
      <DocCode label="A springy pop-in" language="tsx" code={"function Badge() {\n  const frame = useCurrentFrame();\n  const { fps } = useVideoConfig();\n  const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });\n  return (\n    <Rect x={960} y={540} anchorX={0.5} anchorY={0.5} scale={scale}\n      width={320} height={120} cornerRadius={60} fill=\"#78618e\" />\n  );\n}"} />
      <p>Tune the feel with <code>config</code>: <code>stiffness</code> (default 100), <code>damping</code> (default 10), <code>mass</code> (default 1), and <code>overshootClamping</code>. Use <code>from</code> and <code>to</code> to map the result to another range.</p>
      <h3>Place a scene on the timeline</h3><p>Import <code>Sequence</code> to delay or limit a group of layers. This shows the <code>Title</code> component from the previous example starting at frame 30 for 90 frames:</p>
      <DocCode label="Inside your Composition" language="tsx" code={'<Sequence from={30} durationInFrames={90}>\n  <Title />\n</Sequence>'} />
      <p>Within that sequence, <code>useCurrentFrame()</code> starts at 0 when the sequence begins. Child video and audio use the same local clock. Children contribute neither layers nor audio outside the sequence’s time window.</p>
      <h3>Ready-made entrances and exits</h3><p><code>{'<Transition>'}</code> fades, slides, or scales its children at the start (<code>direction="in"</code>, the default) or end (<code>direction="out"</code>) of the enclosing sequence:</p>
      <DocCode label="Slide a caption in, fade it out" language="tsx" code={'<Sequence from={30} durationInFrames={120}>\n  <Transition type="slide" slideFrom="bottom" durationInFrames={15}\n    easing={Easings.easeOutCubic}>\n    <Transition type="fade" direction="out" durationInFrames={20}>\n      <Caption />\n    </Transition>\n  </Transition>\n</Sequence>'} />
      <p>Slides travel <code>distance</code> pixels (64 by default); scales start from <code>scaleFrom</code> (0.8 by default). Pass several types to combine them, such as <code>{"type={['fade', 'slide']}"}</code>.</p>
      <Note title="Keep animation tied to the frame">Compute visual changes from frame or time values so scrubbing and exporting can reproduce each moment. A browser timer or <code>Math.random()</code> is not the composition’s clock; derive “random” values from the frame or an index instead, with <a href="/docs/math/"><code>@celesta/math</code></a>’s seeded <code>random</code> and <code>noise</code>.</Note>
    </>,
  'motion-toolkit': <>
      <p>These helpers cover the patterns that come up in almost every video: scenes in a row, lists that cascade in, and cuts on the beat. Each is built on <code>Sequence</code>, <code>Group</code>, and the frame, so everything stays scrubbable and deterministic. Text effects, the camera, and line drawing are on <a href="/docs/text-camera-lines/">the next page</a>; seeded randomness and noise are in <a href="/docs/math/"><code>@celesta/math</code></a>.</p>
      <h3>Scenes back to back</h3><p><code>{'<Series>'}</code> plays its <code>{'<Series.Sequence>'}</code> children one after another, so you write each scene’s length instead of its start frame. A negative <code>offset</code> overlaps a scene with the one before it. <code>computeSeries()</code> does the same arithmetic without rendering, which is handy for the composition’s total length:</p>
      <DocCode label="Three scenes, sized from their lengths" language="tsx" code={"const SCENES = [\n  { name: 'intro', durationInFrames: 90, Scene: Intro },\n  { name: 'body', durationInFrames: 240, Scene: Body },\n  { name: 'outro', durationInFrames: 60, Scene: Outro },\n];\nconst { durationInFrames } = computeSeries(SCENES);\n\nexport default function Root() {\n  return (\n    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>\n      <Series>\n        {SCENES.map(({ name, durationInFrames, Scene }) => (\n          <Series.Sequence key={name} durationInFrames={durationInFrames}>\n            <Scene />\n          </Series.Sequence>\n        ))}\n      </Series>\n    </Composition>\n  );\n}"} />
      <h3>Cascades and entrances</h3><p><code>progress(frame, start, durationInFrames, easing?)</code> is a clamped 0–1 value for “how far through this span are we”: the building block of most entrances. <code>{'<Stagger each={n}>'}</code> starts each child <code>n</code> frames after the previous one, so an entrance written for frame 0 cascades down a list:</p>
      <DocCode label="Rows that arrive one after another" language="tsx" code={"function Row({ label, y }: { label: string; y: number }) {\n  const frame = useCurrentFrame(); // 0 when this row starts\n  const p = progress(frame, 0, 20, Easings.easeOutExpo);\n  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;\n}\n\n<Stagger each={4}>\n  {items.map((item, i) => <Row key={item} label={item} y={200 + i * 64} />)}\n</Stagger>"} />
      <h3>On the beat</h3><p><code>useBeat({'{ bpm }'})</code> returns the current <code>beat</code>, <code>bar</code>, <code>beatInBar</code>, the <code>progress</code> through the beat, and a <code>pulse</code> that is 1 on every beat and decays until the next. Use it to flash, bump, or cut in time with the soundtrack; <code>offset</code> moves the first beat.</p>
      <DocCode label="A dot that pulses with the music" language="tsx" code={"const { pulse } = useBeat({ bpm: 120 });\nreturn <Rect width={40} height={40} cornerRadius={20} scale={1 + 0.3 * pulse} fill=\"#7cf29c\" />;"} />
      <h3>Cues</h3><p><code>useCue(cues)</code> takes a list of <code>{'{ at, …data }'}</code> objects sorted by <code>at</code> and returns the one in effect, its <code>index</code>, the <code>frame</code> since it started, and the <code>previous</code> and <code>next</code> cues. Swapping captions, a camera moving between stops, or a chart callout that changes are all a list of cues. <code>cueAt(cues, frame)</code> is the same outside a component.</p>
      <DocCode label="Captions that swap and fade in" language="tsx" code={"const active = useCue([\n  { at: 0, text: 'Write it.' },\n  { at: 45, text: 'Preview it.' },\n  { at: 90, text: 'Ship it.' },\n]);\nif (!active) return null;\nreturn <Text opacity={progress(active.frame, 0, 10)}>{active.cue.text}</Text>;"} />
      <h3>Timecodes</h3><p><code>frameToTimecode(frame, fps)</code>, from <code>@celesta/react</code>, formats <code>HH:MM:SS:FF</code> for an on-screen clock.</p>
    </>,
  'text-camera-lines': <>
      <p>Titles that reveal themselves, a camera that travels, and lines that draw on. Like the helpers on the previous page, these are built on the frame, so scrubbing and exporting reproduce every moment.</p>
      <h3>Text effects</h3>
      <Api caption="Text helpers" rows={[
        [<code>TextReveal</code>, <>Lines of a string slide up from behind their own masks, one after another. Set <code>lineHeight</code>, <code>from</code>, <code>stagger</code>, and <code>durationInFrames</code>; <code>align</code> pivots each line.</>],
        [<code>useTypewriter(text, options)</code>, <>The typed-so-far <code>text</code>, its <code>length</code>, <code>done</code>, and <code>caretVisible</code>: steady while typing, blinking while idle.</>],
        [<code>useCountUp(to, options)</code>, <>A number counting to <code>to</code>, with <code>from</code>, <code>delay</code>, <code>durationInFrames</code>, <code>easing</code>, and <code>decimals</code>.</>],
      ]} />
      <DocCode label="An editorial title reveal" language="tsx" code={"<TextReveal x={160} y={300} lineHeight={200} stagger={6}\n  style={{ fontFamily: 'Archivo Black', fontSize: 210,\n    fill: { type: 'solid', color: '#ecefe8' } }}>\n  {'36 DAYS\\nOF CELESTA'}\n</TextReveal>"} />
      <p>Celesta does not measure text in React, so a caret after typed text needs a monospaced font, where every character has the same advance (0.6 em in JetBrains Mono): put the caret at <code>length × advance</code>.</p>
      <h3>Camera</h3><p><code>{'<Camera>'}</code> looks at a point of a larger world: lay the world out in its own coordinates, then animate <code>x</code>/<code>y</code> (the point shown at the center), <code>zoom</code>, and <code>rotation</code>. <code>shake</code> adds a smooth handheld drift of up to that many pixels.</p>
      <DocCode label="Travel along a long timeline" language="tsx" code={"<Camera x={interpolate(frame, [0, 120], [0, 4000], { extrapolateRight: 'clamp' })}\n  y={540} zoom={1.1} shake={4}>\n  <TimelineWorld />\n</Camera>"} />
      <h3>Lines and charts</h3><p><code>{'<Line x1 y1 x2 y2>'}</code> draws a segment and <code>{'<Polyline points>'}</code> connects several; both take <code>stroke</code>, <code>strokeWidth</code>, <code>cap</code> (<code>round</code> by default), and <code>join</code>. Animate a polyline’s <code>progress</code> from 0 to 1 to draw it on, and use <code>pointOnPolyline(points, t)</code> to put a marker on its tip.</p>
      <p>For other shapes, <code>{'<Path>'}</code> takes <code>points</code> (with <code>closed</code>) or SVG-like <code>commands</code> (<code>moveTo</code>, <code>lineTo</code>, <code>quadTo</code>, <code>cubicTo</code>, <code>close</code>), a <code>stroke</code> and/or <code>fill</code>, and <code>cap</code>/<code>join</code>/<code>miterLimit</code>. Each is one layer however many segments it has, stays sharp when scaled, and paints a translucent stroke that crosses itself only once, so draw procedural line art as a few paths rather than many thin rects.</p>
      <h3>Circles, ellipses, and arrows</h3><p><code>{'<Circle radius>'}</code> and <code>{'<Ellipse width height>'}</code> are placed like a <code>Rect</code>: <code>x</code>/<code>y</code> put their box’s anchor (top-left by default), <code>fill</code> gradients start from the box’s top-left, and a <code>stroke</code> stays inside the box. <code>{'<Arrow x1 y1 x2 y2>'}</code> points its head’s tip at <code>x2</code>/<code>y2</code>; set <code>strokeWidth</code>, <code>headLength</code>, <code>headWidth</code>, and <code>heads</code> (<code>end</code>, <code>start</code>, or <code>both</code>). An arrow shorter than its heads shrinks them to fit, so it can grow from nothing, and one with no length draws nothing. A circle is drawn as a rounded <code>Rect</code> and the other two as paths; all three take gradients, transforms, and effects like any layer.</p>
      <DocCode label="A labelled callout" language="tsx" code={"<Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={80}\n  stroke=\"#FFD84D\" strokeWidth={8} />\n<Arrow x1={600} y1={300} x2={890} y2={480} strokeWidth={6} stroke=\"#FFD84D\" />"} />
    </>,
  'layout': <>
      <h3>Coordinates and anchors</h3><p>The canvas origin is the top-left corner, measured in pixels. A layer’s <code>x</code> and <code>y</code> place its <strong>anchor</strong>, which is the top-left corner by default. Set <code>anchorX</code> and <code>anchorY</code> between 0 and 1 to move it: <code>0.5</code> is the center and <code>1</code> the right or bottom edge. Rotation (in degrees) and scale turn around the anchor too.</p>
      <DocCode label="Centered, rotated, and scaled" language="tsx" code={'<Rect x={960} y={540} anchorX={0.5} anchorY={0.5}\n  width={400} height={400} cornerRadius={48}\n  rotation={12} scale={0.8} opacity={0.9} fill="#a68bbf" />'} />
      <p><code>scaleX</code> and <code>scaleY</code> override <code>scale</code> on one axis. <code>opacity</code> multiplies down through groups.</p>
      <h3>Shapes</h3><p><code>Rect</code> draws a rectangle with an optional <code>cornerRadius</code>, <code>fill</code>, and inner <code>stroke</code> with <code>strokeWidth</code>. Colors are hex strings, with an optional alpha channel: <code>#RRGGBB</code> or <code>#RRGGBBAA</code>. A square with a corner radius of half its size makes a circle.</p>
      <h3>Blend modes</h3><p>Set <code>blendMode</code> on any layer to change how it combines with what is beneath it, as CSS <code>mix-blend-mode</code> does: <code>normal</code> (the default), <code>multiply</code>, <code>screen</code>, <code>overlay</code>, <code>add</code>, or <code>difference</code>. On a <code>Group</code>, any mode other than <code>normal</code> draws the children together first, then blends the result as one layer.</p>
      <DocCode label="A HUD that stays readable on light and dark scenes" language="tsx" code={'<Group blendMode="difference" opacity={0.8}>\n  <Text x={48} y={48} style={{ fill: { type: \'solid\', color: \'#ffffff\' } }}>\n    REC 00:00:12\n  </Text>\n</Group>'} />
      <h3>Group and arrange</h3><p><code>Group</code> has no size of its own. Its children are placed relative to its <code>x</code>/<code>y</code>, and its rotation, scale, and opacity apply to all of them. The layout helpers build on it:</p>
      <Api caption="Layout helpers" rows={[
        [<code>Center</code>, <>Moves the origin to the center of the canvas or the enclosing <code>SafeArea</code>.</>],
        [<code>SafeArea</code>, <>Insets its children by <code>padding</code> (a number, or <code>{'{ top, right, bottom, left }'}</code>) and shrinks the area nested helpers see.</>],
        [<code>Stack</code>, <>Places each child <code>spacing</code> pixels apart, <code>vertical</code> by default or <code>horizontal</code>.</>],
        [<code>Grid</code>, <>Places children into <code>columns</code> of <code>columnWidth</code> × <code>rowHeight</code> cells, with optional gaps.</>],
        [<code>Fit</code>, <>Scales content designed at <code>sourceWidth</code> × <code>sourceHeight</code> to <code>contain</code> or <code>cover</code> the current area.</>],
      ]} />
      <DocCode label="A centered list inside a safe margin" language="tsx" code={"<SafeArea padding={96}>\n  <Center>\n    <Stack spacing={96} y={-96}>\n      {['Write', 'Preview', 'Export'].map((step) => (\n        <Text key={step} anchorX={0.5} anchorY={0.5}\n          style={{ fontSize: 64, fill: { type: 'solid', color: '#332f3b' } }}>\n          {step}\n        </Text>\n      ))}\n    </Stack>\n  </Center>\n</SafeArea>"} />
      <p>Helpers position child origins; they do not measure what the children draw. Anchor children at <code>0.5</code> when you want them centered on those points.</p>
      <h3>Clip a group</h3><p>Give a <code>Group</code> a <code>clip</code> to draw its children only inside a rectangle, <code>{'{ x, y, width, height, cornerRadius }'}</code>, in the group’s own coordinates. Use it for a mask reveal, a wipe, or content that scrolls inside a panel. The clip moves, scales, and rotates with the group, its edge is anti-aliased, and clips nested inside one another intersect.</p>
      <DocCode label="Text sliding up from behind an edge" language="tsx" code={"<Group x={120} y={200} clip={{ width: 720, height: 96 }}>\n  <Text y={96 * (1 - reveal)}\n    style={{ fontSize: 88, fill: { type: 'solid', color: '#332f3b' } }}>\n    Layer it.\n  </Text>\n</Group>"} />
    </>,
  'text-fonts': <>
      <h3>Text</h3><p><code>Text</code> renders its children, which must be strings or numbers. Style it with <code>style</code>:</p>
      <DocCode label="An outlined, wrapping caption" language="tsx" code={"<Text x={960} y={900} anchorX={0.5} anchorY={0.5} maxWidth={1400}\n  style={{\n    fontFamily: 'Hiragino Sans',\n    fontSize: 64,\n    fontWeight: 700,\n    lineHeight: 80,\n    align: 'center',\n    fill: { type: 'solid', color: '#ffffff' },\n    stroke: { paint: { type: 'solid', color: '#57456c' }, width: 8 },\n  }}>\n  Words that wrap onto a second line when they reach maxWidth.\n</Text>"} />
      <ul>
        <li><code>fontFamily</code> names a font installed on the computer, or one loaded with <code>{'<Font>'}</code>. When the family has no face at the requested <code>fontWeight</code>, Celesta uses its nearest weight, as CSS does. Characters the font lacks, such as emoji, fall back to another installed font. A JSON project can also bundle a font file as a <code>font</code> asset.</li>
        <li><code>lang</code> selects the language used for fallback fonts when <code>fontFamily</code> is omitted or lacks a character. Set <code>{'<Composition lang="ja-JP">'}</code> for the whole composition. Children inherit the nearest <code>Composition</code>, <code>Group</code>, or <code>Sequence</code> with a <code>lang</code> prop, including subtitles, project timeline text, <code>useTextMetrics()</code>, <code>useFitText()</code>, and component previews. These scopes can change language between frames; measurement and rendering use the same value. <code>{'<Text lang="zh-Hant">'}</code> overrides ancestors, and an explicit <code>style.lang</code> takes priority over that prop. An empty <code>lang</code> selects the system locale. Tags such as <code>ja-JP</code>, <code>ko-KR</code>, <code>zh-Hans</code>, <code>zh-Hant</code>, and <code>zh-Hant-HK</code> are supported. When neither sets a language, Celesta uses the system locale; installed or bundled fonts still determine the available glyphs. The browser preview applies <code>lang</code> in browsers supporting Canvas language selection.</li>
      </ul>
      <DocCode label="A Japanese default with a Chinese group" language="tsx" code={"<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang=\"ja-JP\">\n  <Text>漢字とかな</Text>\n  <Group lang=\"zh-Hant\">\n    <Text y={80}>繁體中文</Text>\n    <Text y={160} lang=\"ko-KR\">한국어</Text>\n  </Group>\n</Composition>"} />
      <p><code>prepare()</code> runs before <code>{'<Composition>'}</code> is mounted. Pass <code>lang</code> explicitly to <code>measureText()</code> there, such as <code>{"measureText('漢字とかな', { lang: 'ja-JP' })"}</code>.</p>
      <p>To use a font file that is not installed, declare it with <code>{'<Font>'}</code>. A relative <code>src</code> is resolved against the entry file’s folder, and <code>fontFamily</code> uses the family name stored inside the file, not the file name. TrueType, OpenType, WOFF, and WOFF2 files all work, and <code>src</code> can be an <code>http://</code> or <code>https://</code> URL.</p>
      <DocCode label="Load a font file next to the entry" language="tsx" code={"<Composition width={1920} height={1080} fps={30} durationInFrames={90}>\n  <Assets>\n    <Font src=\"./fonts/MPLUSRounded1c-Bold.ttf\" />\n  </Assets>\n  <Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n    Hello\n  </Text>\n</Composition>"} />
      <p><code>src</code> can also be a web font stylesheet, such as a Google Fonts CSS link. Celesta loads every font file its <code>@font-face</code> rules list, and <code>fontFamily</code> can use the stylesheet’s family name. Include every weight you use in the link: a weight the stylesheet does not provide is drawn with the nearest weight it does.</p>
      <DocCode label="Load a Google Fonts family" language="tsx" code={"<Assets>\n  <Font src=\"https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700\" />\n</Assets>\n<Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n  こんにちは\n</Text>"} />
      <ul>
        <li><code>maxWidth</code> wraps text at word boundaries, and <code>align</code> positions each line within that width. Use <code>\n</code> in a string to break a line yourself.</li>
        <li>Japanese normally wraps between most characters, splitting a word such as フレーム, while line-break rules keep punctuation such as 、 and 。 off the start of a line. Set <code>lineBreak: 'phrase'</code> in the style to wrap only between phrases, found with <a href="https://github.com/google/budoux">BudouX</a>’s Japanese model; <code>lang</code> currently selects fallback fonts only. A phrase wider than <code>maxWidth</code> wraps inside itself as normal text does. The browser preview ignores <code>lineBreak</code> and wraps only at whitespace.</li>
        <li>Single-line text is anchored vertically by its visible glyphs, so <code>anchorY={'{0.5}'}</code> centers the letters themselves rather than an invisible line box. Its width is the advance width, so leading and trailing spaces still take up room.</li>
        <li><code>anchorY="baseline"</code> anchors text on its first line’s baseline instead. Text layers with the same <code>y</code> then share a baseline, whatever their letters or font sizes.</li>
      </ul>
    </>,
  'media': <>
      <p><code>Image</code>, <code>Video</code>, and <code>Audio</code> read files through <code>src</code>. Relative paths start from the composition file, so keep media beside it in your project folder.</p>
      <p><code>src</code> can also be an <code>http://</code> or <code>https://</code> URL. Celesta downloads the file the first time it is used and reuses that copy afterwards, including offline. The copy is never refreshed, so change the URL when the remote file changes. <code>preloadMedia()</code> accepts URLs as well. In a JSON project, use <code>{'"source": { "type": "url", "url": "https://…" }'}</code> instead of a file path.</p>
      <DocCode label="media.tsx" language="tsx" code={mediaScene.trim()} />
      <ul>
        <li>An <code>Image</code> or <code>Video</code> is drawn at its own pixel size. Position it with <code>x</code>/<code>y</code> and anchors, and resize it with <code>scale</code> or a <code>Fit</code>. Images can be PNG, JPEG, WebP, or PNM.</li>
        <li><code>Video</code> and <code>Audio</code> accept <code>startFrom</code> (seconds into the file) and <code>playbackRate</code>. <code>Audio</code> also takes <code>volume</code> and <code>muted</code>.</li>
        <li>Media placed inside a <code>Sequence</code> starts when the sequence does and stops when it ends.</li>
        <li><code>volume</code> and an audio <code>playbackRate</code> can be a number or keyframes. Keyframe times are seconds from the start of the enclosing sequence, written as <code>{'{ value, timescale }'}</code>.</li>
      </ul>
      <h3>Size a scene to its media</h3><p>The optional <code>prepare()</code> export runs once before rendering and may be <code>async</code>. In the example, <code>preloadMedia()</code> reads the clip’s length and <code>mediaDurationInFrames()</code> converts it to frames, so the end card always follows the footage.</p>
    </>,
  'timelines': <>
      <Note title="Deprecated">JSON projects are deprecated. Use React compositions for new work. This chapter remains for existing <code>.celesta.json</code> projects.</Note>
      <p>A <code>.celesta.json</code> file describes project settings, assets, characters, tracks, and properties. Use JSON when you want an explicit timeline, or combine it with React for generated content.</p>
      <ul><li><code>settings</code> defines dimensions, frame rate, audio sample rate, and optional duration.</li><li><code>assets</code> registers source media by id: <code>video</code>, <code>audio</code>, <code>image</code>, or <code>font</code>. Media files must be local.</li><li><code>tracks</code> contains video, audio, overlay, or dialogue tracks and their items.</li><li>Each item has a <code>range</code> with a start and duration, plus its content and optional <code>transform</code>, <code>opacity</code>, and <code>blendMode</code>.</li></ul>
      <p>Time is represented as <code>{'{ value, timescale }'}</code>: divide the value by the timescale to get seconds. For example, <code>{'{ "value": 10, "timescale": 1 }'}</code> means ten seconds.</p>
      <p>Copy the project below into <code>project.celesta.json</code> in your project folder, then open it with <strong>File → Open…</strong>.</p><details className="doc-details"><summary>View the complete built-in title project</summary><DocCode label="examples/editor-demo.celesta.json" language="json" code={timelineExample.trim()} /></details>
      <p><code>examples/minimal.celesta.json</code> is an empty starting point. For a visible result, begin with the title project above, then change the text and save. Use <strong>File → Reload</strong> to update the preview.</p>
      <h3>Animate with keyframes</h3><p>In a JSON transform, <code>position</code> places the item’s center. Any transform value, <code>opacity</code>, and audio <code>volume</code> can be a fixed number or keyframes. Keyframe times are relative to the item’s start, and <code>easing</code> shapes the curve into that keyframe:</p>
      <DocCode label="Fade an item in over half a second" language="json" code={'"opacity": {\n  "type": "keyframes",\n  "keyframes": [\n    { "time": { "value": 0, "timescale": 30 }, "value": 0 },\n    { "time": { "value": 15, "timescale": 30 }, "value": 1, "easing": "ease-out" }\n  ]\n}'} />
      <p>Easing names match the React <code>Easings</code> in kebab case, such as <code>ease-in-out-cubic</code> or <code>ease-out-back</code>.</p>
      <h3>Combine a project with React</h3><p>A React composition can include <code>{'<ProjectTimeline />'}</code>. Pass its companion JSON project when exporting so Celesta can resolve the timeline:</p>
      <details className="doc-details"><summary>View the companion React composition</summary><DocCode label="with-project.tsx" language="tsx" code={withProject.trim()} /></details><p>Save this as <code>with-project.tsx</code> beside your JSON file, then use the combined-composition command in <a href="/docs/export/">Export a video</a>. To draw a single track instead of the whole timeline, use <code>{'<ProjectTrack id="…" />'}</code>.</p>
    </>,
  'dialogue': <>
      <p>Dialogue scenes are built from three pieces. Each works in React and in JSON projects:</p>
      <ul>
        <li>A <strong>character</strong> defines a portrait (one image per expression, or a layered PSD), optional lip-sync mouths, and how that character’s subtitles look.</li>
        <li>A <strong>character view</strong> places the portrait on screen.</li>
        <li>A <strong>dialogue line</strong> shows a subtitle, plays a voice recording, and can change the view’s expression or mouth while it is on screen.</li>
      </ul>
      <h3>Your first conversation in React</h3><p>Save the file below with two portrait images and two voice recordings beside it:</p>
      <DocCode label="dialogue.tsx" language="tsx" code={dialogueScene.trim()} />
      <ol>
        <li><strong>Declare the character</strong> inside <code>{'<Assets>'}</code>. The ref lets views refer to it. <code>subtitle</code> takes the same position and style props as <code>Text</code>.</li>
        <li><strong>Place a view</strong> with <code>{'<CharacterView>'}</code>. It shows the <code>defaultExpression</code> unless a line says otherwise, and takes <code>x</code>, <code>y</code>, <code>scale</code>, and anchors like any layer.</li>
        <li><strong>Time each line</strong> by wrapping <code>{'<Dialogue>'}</code> in a <code>Sequence</code>. The subtitle, voice, and <code>expression</code> apply only while it is active, then the view returns to its default.</li>
      </ol>
      <Note title="One view, many lines">A <code>Dialogue</code> points to a view, not to the character, so the same portrait stays in place while lines change around it. Add a second <code>Character</code> and view for a two-person conversation.</Note>
      <p>Adjust a line with <code>volume</code>, <code>startFrom</code>, <code>playbackRate</code>, or <code>muted</code>, just as on <code>Audio</code>. Setting <code>x</code>, <code>y</code>, or <code>opacity</code> on a <code>Dialogue</code> moves or fades its subtitle, which is handy inside a <code>Transition</code>.</p>
      <h3>Automatic lip sync</h3><p><code>loadLipSync()</code> listens to a voice recording and spreads the vowels of its transcript across the spoken parts, producing a mouth shape (<code>a</code>, <code>i</code>, <code>u</code>, <code>e</code>, <code>o</code>, or <code>closed</code>) for every moment. Call it from <code>prepare()</code> and pass the result to a <code>Dialogue</code> or <code>CharacterView</code> as <code>lipSync</code>.</p>
      <DocCode label="lip-sync.tsx" language="tsx" code={lipSyncScene.trim()} />
      <ul>
        <li><strong>Voices</strong> must be uncompressed WAV files. Write the transcript in kana or romaji so its vowels can be read; kanji are skipped.</li>
        <li><strong>PSD portraits</strong> name their mouth layers by path, such as <code>face/mouth/a</code>. Layered “tachie” files usually save every folder hidden, so pick visible layers with <code>layers</code>: an array of layer paths, a PSDTool layer-state string, or a <code>.pfv</code> favorite loaded with <code>loadPsdPreset()</code>.</li>
        <li><strong>Image portraits</strong> can lip-sync too. Give <code>portrait.lipSync</code> one transparent mouth image per shape, the same size as the portrait, so each lines up when drawn over it.</li>
        <li>To drive a mouth yourself, pass <code>mouth="a"</code> (or another shape) to a <code>CharacterView</code> or <code>Dialogue</code>. <code>useLipSync(track)</code> returns the current shape if you want to react to it elsewhere.</li>
      </ul>
      <h3>Dialogue in a JSON project</h3><p>A JSON project declares characters in <code>characters</code> and lines as items on a <code>dialogue</code> track. The portrait and subtitle transforms position their centers:</p>
      <details className="doc-details"><summary>View the complete JSON dialogue project</summary><DocCode label="dialogue.celesta.json" language="json" code={dialogueProject.trim()} /></details>
      <p>JSON lines can lip-sync as well: add mouth-image asset ids under the portrait’s <code>lipSync</code>, and list timed shapes on the line, such as <code>{'"lipSync": [{ "time": { "value": 0, "timescale": 30 }, "shape": "a" }]'}</code>. Cue times are relative to the line’s start. For automatic timing from the recording, use React’s <code>loadLipSync()</code>.</p>
      <h3>See it with real artwork</h3><p>The repository’s <code>examples/voiceroid.celesta.json</code> is a complete JSON dialogue project with a sample portrait and voice, and <code>packages/react/examples/character-lipsync-demo.tsx</code> animates a full PSD character with automatic lip sync. On <a href={repository}>GitHub</a>, choose <strong>Code → Download ZIP</strong>, extract the archive, and open either file with <strong>File → Open…</strong>. You do not need to build the source.</p>
    </>,
  'data': <>
      <h3>Load data before rendering</h3><p>Each frame is rendered synchronously, so a component cannot wait for a file or a network request. Do that work in an <code>async prepare()</code> export instead. It runs once, before the first frame, in both the preview and exports. Store the results in module-level variables, then read them while rendering:</p>
      <DocCode label="Fetch once, use on every frame" language="tsx" code={"let headlines: string[] = ['Hello from Celesta'];\n\nexport async function prepare() {\n  try {\n    const response = await fetch('https://example.com/headlines.json');\n    headlines = (await response.json()) as string[];\n  } catch (error) {\n    console.error('Using the fallback headline', error);\n  }\n}"} />
      <p>Keep a fallback value so the scene still renders offline. <code>loadLipSync()</code>, <code>loadPsdPreset()</code>, and <code>preloadMedia()</code> belong in <code>prepare()</code> for the same reason.</p>
      <h3>Reuse a scene with project properties</h3><p>A composition can read values from a JSON project’s <code>properties</code>, so the same scene can be reused with different text or colors. Import the project file, wrap your scene in a <code>ProjectProvider</code>, and read values with <code>useProjectProperty(key, fallback)</code>:</p>
      <DocCode label="Read a title from project.celesta.json" language="tsx" code={"import type { Project } from '@celesta/react';\nimport projectFile from './project.celesta.json';\n\nconst project = projectFile as Project;\n\nfunction Title() {\n  const title = useProjectProperty('title', 'Chapter 1');\n  const accent = useProjectProperty('accent', '#a68bbf');\n  return (\n    <Text x={960} y={540} anchorX={0.5} anchorY={0.5}\n      style={{ fontSize: 96, fill: { type: 'solid', color: accent } }}>\n      {title}\n    </Text>\n  );\n}\n\nexport default function Root() {\n  return (\n    <Composition width={1920} height={1080} fps={30} durationInFrames={90}>\n      <ProjectProvider project={project}>\n        <Title />\n      </ProjectProvider>\n    </Composition>\n  );\n}"} />
      <p>Change <code>{'"properties": { "title": "Chapter 3" }'}</code> in the JSON file, and the scene follows. Call <code>defineProjectProperties()</code> at the top level of the file to describe each property’s type (<code>string</code>, <code>number</code>, <code>boolean</code>, <code>color</code>, or <code>select</code>), label, and default for Celesta’s Inspector.</p>
      <h3>Use React components from a JSON timeline</h3><p>Register a component by name, and a JSON item with <code>{'"content": { "type": "component", "component": "LowerThird", "props": { … } }'}</code> will render it through <code>{'<ProjectTimeline />'}</code>:</p>
      <DocCode label="Register a component" language="tsx" code={"type LowerThirdProps = { name: string; role: string };\n\nfunction LowerThird({ name, role }: LowerThirdProps) {\n  return <Text style={{ fontSize: 48 }}>{`${name} · ${role}`}</Text>;\n}\n\nregisterComponent<LowerThirdProps>('LowerThird', LowerThird, {\n  name: { type: 'string', defaultValue: 'Mira' },\n  role: { type: 'string', defaultValue: 'Host' },\n});"} />
      <p>Call <code>registerComponent()</code> at the top level of your file. The item’s <code>range</code>, <code>transform</code>, and <code>opacity</code> still come from the JSON timeline. Component props must be plain JSON values; use a <code>type</code> alias rather than an <code>interface</code> for them.</p>
    </>,
  'export': <>
      <h3>In your browser</h3><p>Open the <a href="/#playground">web editor</a>, change or open a TSX composition, and choose <strong>Export MP4</strong>. The source, preview, and export use Celesta’s React scene evaluator. Add local media files by name with <strong>Add media</strong>. The browser needs H.264 WebCodecs support; audio also needs AAC encoding. The browser preview is silent, while the exported MP4 includes constant-rate audio clips. For PSDs, project timelines, <code>prepare()</code>, third-party imports, animated audio automation, and native-renderer output, use the installed app or CLI.</p>
      <h3>From the app</h3><p>Choose <strong>Export…</strong> and select an MP4 destination. For a section of the composition, press <kbd>I</kbd> and <kbd>O</kbd> to mark the start and end. The status bar shows progress; <strong>Cancel export</strong> stops the job.</p>
      <h3>From the command line</h3><ExportCommands />
      <p>Time values accept seconds, <code>MM:SS.mmm</code>, or <code>HH:MM:SS.mmm</code>. The selected span becomes a new video starting at its own 00:00.</p>
      <h3>On Linux without a GPU</h3><p>On Linux, the exporter renders through Vulkan. On machines without a GPU, such as CI runners and cloud containers, install Mesa’s software Vulkan driver (lavapipe), which renders on the CPU. This works with a <a href="/docs/build-from-source/">source build</a>:</p>
      <DocCode label="Install software Vulkan" code="sudo apt-get install -y mesa-vulkan-drivers" />
      <p>Software rendering is much slower than a GPU: with 4 CPU cores, 1080p compositions render at roughly 10 to 60 frames per second, depending on the scene. Check a single frame as a PNG before exporting the whole video. From the repository root:</p>
      <DocCode label="Export one frame" code="cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx --frame 0 frame.png" />
      <p>Minimal containers often have no fonts installed, and text cannot be drawn without at least one. Install some, such as <code>fonts-dejavu-core</code> (and <code>fonts-noto-cjk</code> for Japanese), or load font files with <code>{'<Font>'}</code>.</p>
      <p>On a software renderer, <code>--color-conversion auto</code> leaves the RGB-to-YUV conversion to the encoder. <code>--render-quality draft</code> is quicker for checking timing, but not pixels. To export in a container without building Celesta yourself, build the <a href={`${repository}/tree/main/packaging/linux`}>Linux container image</a> from the repository root, then run it from your project folder, which is mounted at <code>/work</code>. <code>--user</code> makes the output belong to you instead of root:</p>
      <DocCode label="Build the container image" code="docker build -f packaging/linux/Dockerfile -t celesta-exporter ." />
      <DocCode label="Export in a container" code={'docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4'} />
      <Note title="Before you render">MP4 output uses H.264 video and AAC audio. Width and height must be non-zero, even numbers. Keep every referenced local media file available during export.</Note>
    </>,
  'reference': <>
      <p>Import these APIs from <code>@celesta/react</code>. Random numbers, noise, and other math helpers come from <code>@celesta/math</code> (see <a href="/docs/math/">its guide</a>), and syntax-highlighted code from <code>@celesta/code</code> (see <a href="/docs/code/">its guide</a>). Celesta’s TypeScript declarations provide the complete prop types and editor completions.</p>
      <Api caption="Layers" rows={[
        [<code>Composition</code>, <>Set <code>width</code>, <code>height</code>, <code>fps</code>, and <code>durationInFrames</code>. Optional <code>lang</code> supplies the default text language. Return exactly one from your default export.</>],
        [<code>Rect</code>, <>A rectangle with <code>width</code>, <code>height</code>, and optional <code>fill</code>, <code>stroke</code>, <code>strokeWidth</code>, and <code>cornerRadius</code>.</>],
        [<code>Text</code>, <>Text from string or number children, styled with <code>style</code> and wrapped with <code>maxWidth</code>. Optional <code>lang</code> selects its language; <code>style.lang</code> takes priority. See <a href="/docs/text-fonts/">Text & fonts</a>.</>],
        [<code>Group</code>, <>Applies a shared position, scale, rotation, and opacity to child layers. Optional <code>lang</code> sets their default text language.</>],
        [<><code>Image</code> / <code>Video</code> / <code>Audio</code></>, <>Local media through <code>src</code>. See <a href="/docs/media/">Images, video & sound</a>.</>],
        [<code>Font</code>, <>Loads a local font file through <code>src</code> so <code>Text</code> can use its family. See <a href="/docs/text-fonts/">Text & fonts</a>.</>],
      ]} />
      <Api caption="Time and motion" rows={[
        [<code>Sequence</code>, <>Places children at a frame offset with <code>from</code> and an optional <code>durationInFrames</code>. Optional <code>lang</code> sets their default text language.</>],
        [<code>Transition</code>, <>A fade, slide, or scale (or several at once) at the start or end of the enclosing sequence.</>],
        [<><code>Series</code> / <code>computeSeries</code></>, <>Scenes back to back, by length. See <a href="/docs/motion-toolkit/">Scenes, cues & beats</a>.</>],
        [<code>Stagger</code>, <>Starts each child a fixed number of frames after the previous one.</>],
        [<code>progress</code>, <>A clamped, eased 0–1 value for a span of frames.</>],
        [<><code>useBeat()</code> / <code>beatAt</code></>, <>Beats, bars, and a pulse for a tempo in BPM.</>],
        [<><code>useCue()</code> / <code>cueAt</code></>, <>The cue in effect from a list of <code>{'{ at, …data }'}</code>.</>],
        [<><code>TextReveal</code>, <code>useTypewriter()</code>, <code>useCountUp()</code></>, <>Masked line reveals, typing, and counting numbers.</>],
        [<code>Camera</code>, <>Look at a point of a larger world, with zoom, rotation, and shake.</>],
        [<><code>Line</code> / <code>Polyline</code> / <code>Path</code> / <code>pointOnPolyline</code></>, <>Segments, curves, and filled shapes; polylines can draw themselves on.</>],
        [<><code>Circle</code> / <code>Ellipse</code> / <code>Arrow</code></>, <>Diagram shapes: a circle is a rounded <code>Rect</code>, ellipses and arrows are <code>Path</code>s; circles and ellipses are placed like a <code>Rect</code>.</>],
        [<><code>interpolate</code> / <code>Easings</code></>, <>Map a frame to a value, with easing and extrapolation.</>],
        [<code>interpolateColor</code>, <>Map a frame to a <code>#RRGGBBAA</code> color, blending with premultiplied alpha.</>],
        [<code>spring</code>, <>A physics-based value that settles from 0 to 1.</>],
        [<><code>useCurrentFrame()</code> / <code>useCurrentTime()</code></>, <>The current frame, or the exact time, local to an enclosing sequence.</>],
        [<code>useVideoConfig()</code>, <>The composition’s (or sequence’s) dimensions, frame rate, and duration.</>],
        [<><code>timecodeToFrame()</code> / <code>frameToTimecode()</code></>, <>Convert a <code>MM:SS.mmm</code>-style timecode to a frame number, and format a frame as <code>HH:MM:SS:FF</code>.</>],
      ]} />
      <Api caption="Characters" rows={[
        [<><code>Assets</code> / <code>Character</code></>, <>Declare a character’s portrait, expressions, lip-sync mouths, and subtitle style.</>],
        [<code>CharacterView</code>, <>Places a character’s portrait. Accepts <code>expression</code>, <code>mouth</code>, and <code>lipSync</code>.</>],
        [<code>Dialogue</code>, <>Shows a line as a subtitle, plays its <code>audio</code>, and changes the view’s expression while active.</>],
        [<><code>loadLipSync</code> / <code>useLipSync</code></>, <>Generate mouth shapes from a WAV recording, and read the current shape.</>],
        [<code>loadPsdPreset</code>, <>Load visible PSD layers from a PSDTool <code>.pfv</code> favorite.</>],
      ]} />
      <Api caption="Layout, data, and tools" rows={[
        [<><code>Center</code>, <code>SafeArea</code>, <code>Stack</code>, <code>Grid</code>, <code>Fit</code></>, <>Arrange child layers. See <a href="/docs/layout/">layout helpers</a>.</>],
        [<><code>preloadMedia</code> / <code>mediaDurationInFrames</code></>, <>Read a media file’s length and size in <code>prepare()</code>.</>],
        [<><code>loadProject</code>, <code>ProjectProvider</code>, <code>useProjectProperty</code></>, <>Read values from a JSON project.</>],
        [<><code>ProjectTimeline</code> / <code>ProjectTrack</code></>, <>Draw a companion JSON project’s timeline, or one of its tracks.</>],
        [<><code>registerComponent</code> / <code>defineProjectProperties</code></>, <>Make components and properties available to JSON projects and the Inspector.</>],
        [<><code>DebugOverlay</code> / <code>DebugBounds</code> / <code>useIsPreview()</code></>, <>Preview-only guides that never appear in an export.</>],
      ]} />
    </>,
  'examples': <>
      <div className="doc-example-grid">
        <a href="/docs/react-compositions/"><span>01 / REACT</span><strong>A title in motion</strong><p>Follow the complete fading-title example in this guide.</p></a>
        <a href="/docs/media/"><span>02 / MEDIA</span><strong>Footage and sound</strong><p>Combine Image, Video, and Audio layers in one scene.</p></a>
        <a href="/docs/dialogue/"><span>03 / DIALOGUE</span><strong>Give it a voice</strong><p>Pair a portrait, subtitles, and a voice with lip sync.</p></a>
      </div>
      <h3>Included with the app</h3><p>Copy an example into your own project folder before editing it.</p><ul><li><strong>macOS:</strong> in Finder, right-click Celesta in Applications, choose <strong>Show Package Contents</strong>, then open <code>Contents/Resources/examples</code>.</li><li><strong>Windows:</strong> open the <code>examples</code> folder beside <code>Celesta.exe</code> in the installed or portable app folder.</li></ul>
      <p>The repository’s <code>packages/react/examples</code> directory contains more scenes covering text, animation, layout, dialogue, lip sync, and editable project properties. Download the source archive, extract it, and open a supported entry through <strong>File → Open…</strong> in the installed app; no source build is needed.</p>
      <a className="doc-text-link" href={`${repository}/tree/main/packages/react/examples`}>Browse all source examples on GitHub ↗</a>
      <p>You can also <a href="/#playground">open the web editor</a>, edit or import a self-contained <code>.tsx</code> composition, preview frames, and export MP4 directly. Download the source to keep editing it in Celesta.</p>
    </>,
  'build-from-source': <SourceBuild />,
  'troubleshooting': <>
      <details className="doc-details" open><summary>The app cannot find its bundled runtime</summary><p>On macOS, copy the complete app into Applications before launching it. For a portable Windows build, extract the entire archive and keep <code>runtime</code> and the DLLs beside <code>Celesta.exe</code>. If files are missing, extract or install the package again.</p></details>
      <details className="doc-details"><summary>A source build cannot find FFmpeg</summary><p>Confirm that the 8.1.x development libraries are installed, not just the executable. On macOS, set <code>PKG_CONFIG_PATH</code> in the same shell where you run Cargo. On Windows, check <code>VCPKG_ROOT</code> and the MSVC build tools. On Linux, point <code>PKG_CONFIG_PATH</code> at the FFmpeg you built, such as <code>/opt/ffmpeg8/lib/pkgconfig</code>. See <a href="/docs/build-from-source/">source-build setup</a>.</p></details>
      <details className="doc-details"><summary>My media is missing</summary><p>Celesta currently supports local media files, not remote media URLs. Check the paths in your project, keep the referenced files alongside the project when sharing it, and look for missing-asset indicators in the Assets panel.</p></details>
      <details className="doc-details"><summary>The preview did not change after saving</summary><p>React compositions reload automatically. For JSON projects, use <strong>File → Reload</strong>. Make sure you saved the file you actually opened in Celesta; the app itself does not edit the source.</p></details>
      <details className="doc-details"><summary>My text uses the wrong font</summary><p><code>fontFamily</code> must match the family name of a font installed on the computer that renders the video, or of a font file loaded with <code>{'<Font>'}</code>. When it does not, Celesta falls back to another font and warns: the app lists the family with the preview’s warnings, and <code>celesta-export</code> prints a warning that names the family, weight, and text layer. Check the exact family name in your system’s font manager. To avoid installing the font on every machine that exports the project, keep the font file next to the entry and load it with <code>{'<Font>'}</code>.</p></details>
      <details className="doc-details"><summary>The mouth does not move</summary><p>Lip sync needs an uncompressed WAV voice and a transcript with readable vowels (kana or romaji). Check that <code>loadLipSync()</code> runs in <code>prepare()</code>, that the track is passed as <code>lipSync</code>, and that each mouth path matches a layer in the PSD exactly. For a PSD that shows nothing, set <code>layers</code> from a PSDTool preset.</p></details>
      <details className="doc-details"><summary>My editor cannot resolve @celesta/react</summary><p>Open the composition in Celesta and choose <strong>File → Set Up TypeScript</strong>. Check that your <code>tsconfig.json</code> extends <code>./.celesta/tsconfig.json</code>. When building from source, also complete the React package’s install, codegen, and build steps.</p></details>
      <details className="doc-details"><summary>MP4 export will not start</summary><p>Use non-zero, even-numbered dimensions and confirm that all local media is available. If the destination file already exists, choose another path or explicitly add <code>--overwrite</code> to the CLI command.</p></details>
      <h3>Still stuck?</h3><p>Open an issue with your operating system, reproduction steps, and the exact error message. Include a small project and media you have permission to share when possible.</p>
      <a className="doc-text-link" href={`${repository}/issues`}>Report a problem on GitHub ↗</a>
    </>,
};
