import { useState, type ReactNode } from 'react';
import { DocCode } from './DocCode';
import { allDownloads, downloads, repository } from './links';
import firstScene from './examples/first-scene.tsx?raw';
import dialogueScene from './examples/dialogue.tsx?raw';
import dialogueProject from './examples/dialogue.celesta.json?raw';
import lipSyncScene from './examples/lip-sync.tsx?raw';
import mediaScene from './examples/media.tsx?raw';
import timelineExample from '../../../examples/editor-demo.celesta.json?raw';
import withProject from '../../react/examples/with-project.tsx?raw';

export { repository };

function Note({ title, children }: { title: string; children: ReactNode }) {
  return <aside className="doc-note"><strong>{title}</strong><div>{children}</div></aside>;
}

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
      {platform === 'Linux' && <><h4>Linux</h4><p>Install <code>pkg-config</code> and the FFmpeg development packages using your distribution’s package manager. On distributions that use Debian-style package names, these are:</p><DocCode label="Linux development packages" language="text" code={'libavcodec-dev libavformat-dev libavfilter-dev\nlibavdevice-dev libavutil-dev libswscale-dev libswresample-dev'} /><p>Your distribution must provide FFmpeg 8.1.x. GPUI may also require native window-system and graphics development packages for your environment.</p></>}
    </div>
    <h3>2. Get the source</h3>
    <DocCode label="Clone Celesta" code={`git clone ${repository}.git celesta\ncd celesta`} />
    <h3>3. Open your first project</h3>
    <DocCode label="Launch the native app" code="cargo run -p celesta-editor --release" />
    <p>Celesta opens its built-in demo. You should see a preview, timeline, Assets panel, and Inspector. You can also pass a project path:</p>
    <DocCode label="Open the example project" code="cargo run -p celesta-editor --release -- examples/editor-demo.celesta.json" />
    <h3>4. Build the React runtime</h3><p>For React compositions in a source build, install the package dependencies, generate the shared types, and build the runtime:</p>
    <DocCode label="Build the React runtime from source" code={'cd packages/react\npnpm install\npnpm run codegen\npnpm run build\ncd ../..'} />
    <DocCode label="Preview React from source" code="cargo run -p celesta-editor --release -- packages/react/examples/title.tsx" />
    <DocCode label="Export React from source" code="cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx output.mp4" />
    <p>Run Cargo commands from the repository root. Packaged-app users can skip this entire section and go straight to their <a href="#react-compositions">first React composition</a>.</p>
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
      {platform === 'Linux' && <><h4>Linux</h4><p>Use the <a href="#build-from-source">source-build instructions</a> for Linux. Check the release assets for any additional platform packages provided with a release.</p></>}
    </div>
    <h3>2. Create a place for your project</h3><p>Make a folder in your Documents directory, such as <code>my-video</code>. Keep your composition files and media there, separate from the installed app.</p>
    <h3>3. Open your first scene</h3><p>Celesta opens a built-in demo on launch. To make your own, save the <a href="#react-compositions">React title example</a> as <code>first-scene.tsx</code> in your project folder, then choose <strong>File → Open…</strong> in Celesta and select it.</p>
    <p>Prefer an existing example? The app package includes <code>title.tsx</code>, <code>editor-demo.celesta.json</code>, and <code>minimal.celesta.json</code>. See <a href="#examples">where to find them</a>.</p>
    <h3>Updating Celesta</h3><p>Quit Celesta and install the newer release package. For portable Windows builds, extract the new version into its own folder and launch it from there. Keep your projects and media in your Documents directory so they stay separate from app updates.</p>
    <p>Working on Celesta itself? See <a href="#build-from-source">Build from source</a> for the developer setup.</p>
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
    <p>To combine React with the <a href="#timelines">JSON timeline example</a>, save both files in your project folder and pass the companion project:</p>
    <DocCode label="Export a combined composition" code={`${executable} --react with-project.tsx --project project.celesta.json output.mp4`} />
    <p>Using a source build? The <a href="#build-from-source">developer instructions</a> include the equivalent Cargo command.</p>
  </>;
}

export interface DocSection { id: string; title: string; description: string; keywords: string; content: ReactNode }

function Api({ rows, caption }: { caption: string; rows: [ReactNode, ReactNode][] }) {
  return <div className="doc-table-wrap doc-api" tabIndex={0} aria-label={caption}><table><caption>{caption}</caption><thead><tr><th>API</th><th>Use it for</th></tr></thead><tbody>
    {rows.map(([name, use], i) => <tr key={i}><td>{name}</td><td>{use}</td></tr>)}
  </tbody></table></div>;
}

export const sections: DocSection[] = [
  {
    id: 'installation', title: 'Install & get started', description: 'Download the app. Open a scene. Start creating.', keywords: 'setup download installer binary macOS Windows Linux dmg portable zip updates' , content: <Installation />,
  },
  {
    id: 'preview', title: 'Preview your work', description: 'Get comfortable with the app, one frame at a time.', keywords: 'keyboard shortcuts open reload play pause scrub inspector audio mute solo', content: <>
      <p>Choose <strong>File → Open…</strong> to open a <code>.celesta.json</code> project or a React composition (<code>.tsx</code>, <code>.jsx</code>, <code>.ts</code>, or <code>.js</code>). Select an asset, track, or clip to inspect its details.</p>
      <Note title="Your source is your canvas">Edit projects and compositions in your text editor. The Celesta app previews and exports them; it does not edit the project itself.</Note>
      <p>React compositions reload when you save. For a JSON project, choose <strong>File → Reload</strong> after changing the source. Keep referenced media files available; missing assets are marked in the Assets panel.</p>
      <div className="doc-table-wrap" tabIndex={0} aria-label="Keyboard shortcuts"><table><caption>Keyboard shortcuts</caption><thead><tr><th>Action</th><th>macOS</th><th>Windows / Linux</th></tr></thead><tbody>
        <tr><td>Open a project</td><td><kbd>⌘ O</kbd></td><td><kbd>Ctrl O</kbd></td></tr>
        <tr><td>Reload a JSON project</td><td><kbd>⌘ R</kbd></td><td><kbd>Ctrl R</kbd></td></tr>
        <tr><td>Play / pause</td><td colSpan={2}><kbd>Space</kbd></td></tr>
        <tr><td>Step backward / forward</td><td colSpan={2}><kbd>←</kbd> / <kbd>→</kbd></td></tr>
        <tr><td>Mark export start / end</td><td colSpan={2}><kbd>I</kbd> / <kbd>O</kbd></td></tr>
      </tbody></table></div>
      <h3>Listen as you look</h3><p>Playback synchronizes the picture and audio. Use waveforms and track levels to check timing, mute or solo tracks to isolate a sound, and adjust the preview volume. Drag along the timeline ruler to scrub to a specific moment.</p>
      <h3>Preview-only guides</h3><p><code>{'<DebugOverlay />'}</code> draws the frame edge, a safe-area inset, and the center point, and <code>{'<DebugBounds>'}</code> outlines a group of layers. Both appear only in the Celesta preview and add nothing to an export, so you can leave them in while you work. <code>useIsPreview()</code> tells your own components the same thing.</p>
    </>,
  },
  {
    id: 'react-compositions', title: 'Your first React composition', description: 'A five-second title, written in familiar components.', keywords: 'tsx jsx components typescript setup codegen build title', content: <>
      <p>The installed app already includes the React runtime. There is no package installation or build step for your composition: just save a file and open it in Celesta.</p>
      <h3>Make your first scene</h3><p>Create <code>first-scene.tsx</code> in your own project folder. This example creates a 1920 × 1080 composition at 30 fps. The title fades in over 30 frames, then holds until the five-second composition ends.</p>
      <DocCode label="first-scene.tsx" language="tsx" code={firstScene.trim()} />
      <p>In Celesta, choose <strong>File → Open…</strong> and select <code>first-scene.tsx</code>. Press <kbd>Space</kbd> to play.</p>
      <p>Change the text or colors and save. The native preview reloads the composition. Celesta components describe video layers: use <code>Text</code>, <code>Rect</code>, and <code>Group</code> rather than HTML elements or CSS classes.</p>
      <h3>How a composition is put together</h3>
      <ul>
        <li>The file’s <strong>default export</strong> returns a single <code>{'<Composition>'}</code> with its size, frame rate, and length.</li>
        <li>Everything inside it is drawn in order: later layers appear on top of earlier ones.</li>
        <li>Your own components are ordinary React function components. Use props, <code>.map()</code>, and conditionals as you would anywhere else.</li>
        <li>An optional named export, <code>prepare()</code>, runs once before the first frame. See <a href="#data">Data, properties & components</a>.</li>
      </ul>
      <h3>Set up TypeScript in your editor</h3><p>With a React composition open, choose <strong>File → Set Up TypeScript</strong>. Celesta copies matching declarations for <code>@celesta/react</code>, React, and Node.js into a local <code>.celesta/</code> directory.</p>
      <p>If there is no <code>tsconfig.json</code>, Celesta creates one. If you already have a configuration, add this <code>extends</code> property while preserving your other settings:</p>
      <DocCode label="tsconfig.json" language="json" code={'{\n  "extends": "./.celesta/tsconfig.json"\n}'} />
      <p>You do not need to install those declaration packages from npm for this editor setup. Celesta refreshes <code>.celesta/</code> when you open the project in a newer app version; that directory ignores itself in Git.</p>
    </>,
  },
  {
    id: 'animation', title: 'Frames, timing & animation', description: 'Make motion a function of the frame.', keywords: 'interpolate easing spring transition sequence hooks useCurrentFrame duration fps opacity animation', content: <>
      <p>A composition’s duration in seconds is <code>durationInFrames / fps</code>. At 30 fps, 150 frames make five seconds, with frame indices from 0 through 149. Use <code>useCurrentFrame()</code> inside a composition to calculate animated values.</p>
      <h3>Interpolate between values</h3><p>The title example uses <code>Math.min(frame / 30, 1)</code> for opacity. For easing and more control, import <code>interpolate</code> and <code>Easings</code> from <code>@celesta/react</code>, then replace the calculation with:</p>
      <DocCode label="An eased fade (inside a component)" language="tsx" code={"const opacity = interpolate(frame, [0, 30], [0, 1], {\n  easing: Easings.easeOut,\n  extrapolateLeft: 'clamp',\n  extrapolateRight: 'clamp',\n});"} />
      <p>Pass the result to <code>{'<Text opacity={opacity} … />'}</code>. Input ranges must be strictly increasing and have the same number of entries as the output range. Extrapolation defaults to <code>extend</code>; use <code>clamp</code> to hold the first and last values.</p>
      <p><code>Easings</code> covers the familiar easings.net curves: <code>linear</code>, and the <code>easeIn</code>/<code>easeOut</code>/<code>easeInOut</code> variants of <code>Sine</code>, <code>Quad</code>, <code>Cubic</code>, <code>Quart</code>, <code>Quint</code>, <code>Expo</code>, <code>Circ</code>, <code>Back</code>, <code>Elastic</code>, and <code>Bounce</code> (for example <code>Easings.easeOutBack</code>).</p>
      <h3>Add a little bounce</h3><p><code>spring()</code> simulates a damped spring: it starts at 0, settles at 1, and may overshoot on the way. It needs the frame rate, so read it from <code>useVideoConfig()</code>:</p>
      <DocCode label="A springy pop-in" language="tsx" code={"function Badge() {\n  const frame = useCurrentFrame();\n  const { fps } = useVideoConfig();\n  const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });\n  return (\n    <Rect x={960} y={540} anchorX={0.5} anchorY={0.5} scale={scale}\n      width={320} height={120} cornerRadius={60} fill=\"#78618e\" />\n  );\n}"} />
      <p>Tune the feel with <code>config</code>: <code>stiffness</code> (default 100), <code>damping</code> (default 10), <code>mass</code> (default 1), and <code>overshootClamping</code>. Use <code>from</code> and <code>to</code> to map the result to another range.</p>
      <h3>Place a scene on the timeline</h3><p>Import <code>Sequence</code> to delay or limit a group of layers. This shows the <code>Title</code> component from the previous example starting at frame 30 for 90 frames:</p>
      <DocCode label="Inside your Composition" language="tsx" code={'<Sequence from={30} durationInFrames={90}>\n  <Title />\n</Sequence>'} />
      <p>Within that sequence, <code>useCurrentFrame()</code> starts at 0 when the sequence begins. Child video and audio use the same local clock. Children contribute neither layers nor audio outside the sequence’s time window.</p>
      <h3>Ready-made entrances and exits</h3><p><code>{'<Transition>'}</code> fades, slides, or scales its children at the start (<code>direction="in"</code>, the default) or end (<code>direction="out"</code>) of the enclosing sequence:</p>
      <DocCode label="Slide a caption in, fade it out" language="tsx" code={'<Sequence from={30} durationInFrames={120}>\n  <Transition type="slide" slideFrom="bottom" durationInFrames={15}\n    easing={Easings.easeOutCubic}>\n    <Transition type="fade" direction="out" durationInFrames={20}>\n      <Caption />\n    </Transition>\n  </Transition>\n</Sequence>'} />
      <p>Slides travel <code>distance</code> pixels (64 by default); scales start from <code>scaleFrom</code> (0.8 by default).</p>
      <Note title="Keep animation tied to the frame">Compute visual changes from frame or time values so scrubbing and exporting can reproduce each moment. A browser timer or <code>Math.random()</code> is not the composition’s clock; derive “random” values from the frame or an index instead.</Note>
    </>,
  },
  {
    id: 'layout', title: 'Shapes, text & layout', description: 'Position layers, style text, and arrange a scene.', keywords: 'Rect Text Group style font Font webfont woff woff2 Google Fonts css fontFamily fontWeight stroke outline lineHeight maxWidth wrap align anchor rotation scale Center SafeArea Stack Grid Fit layout', content: <>
      <h3>Coordinates and anchors</h3><p>The canvas origin is the top-left corner, measured in pixels. A layer’s <code>x</code> and <code>y</code> place its <strong>anchor</strong>, which is the top-left corner by default. Set <code>anchorX</code> and <code>anchorY</code> between 0 and 1 to move it: <code>0.5</code> is the center and <code>1</code> the right or bottom edge. Rotation (in degrees) and scale turn around the anchor too.</p>
      <DocCode label="Centered, rotated, and scaled" language="tsx" code={'<Rect x={960} y={540} anchorX={0.5} anchorY={0.5}\n  width={400} height={400} cornerRadius={48}\n  rotation={12} scale={0.8} opacity={0.9} fill="#a68bbf" />'} />
      <p><code>scaleX</code> and <code>scaleY</code> override <code>scale</code> on one axis. <code>opacity</code> multiplies down through groups.</p>
      <h3>Shapes</h3><p><code>Rect</code> draws a rectangle with an optional <code>cornerRadius</code>, <code>fill</code>, and inner <code>stroke</code> with <code>strokeWidth</code>. Colors are hex strings, with an optional alpha channel: <code>#RRGGBB</code> or <code>#RRGGBBAA</code>. A square with a corner radius of half its size makes a circle.</p>
      <h3>Text</h3><p><code>Text</code> renders its children, which must be strings or numbers. Style it with <code>style</code>:</p>
      <DocCode label="An outlined, wrapping caption" language="tsx" code={"<Text x={960} y={900} anchorX={0.5} anchorY={0.5} maxWidth={1400}\n  style={{\n    fontFamily: 'Hiragino Sans',\n    fontSize: 64,\n    fontWeight: 700,\n    lineHeight: 80,\n    align: 'center',\n    fill: { type: 'solid', color: '#ffffff' },\n    stroke: { paint: { type: 'solid', color: '#57456c' }, width: 8 },\n  }}>\n  Words that wrap onto a second line when they reach maxWidth.\n</Text>"} />
      <ul>
        <li><code>fontFamily</code> names a font installed on the computer, or one loaded with <code>{'<Font>'}</code>. Characters the font lacks, such as emoji, fall back to another installed font. A JSON project can also bundle a font file as a <code>font</code> asset.</li>
      </ul>
      <p>To use a font file that is not installed, declare it with <code>{'<Font>'}</code>. A relative <code>src</code> is resolved against the entry file’s folder, and <code>fontFamily</code> uses the family name stored inside the file, not the file name. TrueType, OpenType, WOFF, and WOFF2 files all work, and <code>src</code> can be an <code>http://</code> or <code>https://</code> URL.</p>
      <DocCode label="Load a font file next to the entry" language="tsx" code={"<Composition width={1920} height={1080} fps={30} durationInFrames={90}>\n  <Assets>\n    <Font src=\"./fonts/MPLUSRounded1c-Bold.ttf\" />\n  </Assets>\n  <Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n    Hello\n  </Text>\n</Composition>"} />
      <p><code>src</code> can also be a web font stylesheet, such as a Google Fonts CSS link. Celesta loads every font file its <code>@font-face</code> rules list, and <code>fontFamily</code> can use the stylesheet’s family name. Include every weight you use in the link: a weight the stylesheet does not provide falls back to another font.</p>
      <DocCode label="Load a Google Fonts family" language="tsx" code={"<Assets>\n  <Font src=\"https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700\" />\n</Assets>\n<Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n  こんにちは\n</Text>"} />
      <ul>
        <li><code>maxWidth</code> wraps text at word boundaries, and <code>align</code> positions each line within that width. Use <code>\n</code> in a string to break a line yourself.</li>
        <li>Single-line text is anchored by its visible glyphs, so <code>anchorY={'{0.5}'}</code> centers the letters themselves rather than an invisible line box.</li>
      </ul>
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
    </>,
  },
  {
    id: 'media', title: 'Images, video & sound', description: 'Bring your own pictures, footage, music, and voices.', keywords: 'Image Video Audio media src url remote download startFrom playbackRate volume muted keyframes fade music preloadMedia mediaDurationInFrames png jpeg webp mp4 wav', content: <>
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
  },
  {
    id: 'timelines', title: 'JSON project timelines', description: 'Describe a project with tracks, clips, and local media.', keywords: 'json schema project assets tracks settings range time timescale properties keyframes easing ProjectTimeline', content: <>
      <p>A <code>.celesta.json</code> file describes project settings, assets, characters, tracks, and properties. Use JSON when you want an explicit timeline, or combine it with React for generated content.</p>
      <ul><li><code>settings</code> defines dimensions, frame rate, audio sample rate, and optional duration.</li><li><code>assets</code> registers source media by id: <code>video</code>, <code>audio</code>, <code>image</code>, or <code>font</code>. Media files must be local.</li><li><code>tracks</code> contains video, audio, overlay, or dialogue tracks and their items.</li><li>Each item has a <code>range</code> with a start and duration, plus its content and optional <code>transform</code> and <code>opacity</code>.</li></ul>
      <p>Time is represented as <code>{'{ value, timescale }'}</code>: divide the value by the timescale to get seconds. For example, <code>{'{ "value": 10, "timescale": 1 }'}</code> means ten seconds.</p>
      <p>Copy the project below into <code>project.celesta.json</code> in your project folder, then open it with <strong>File → Open…</strong>.</p><details className="doc-details"><summary>View the complete built-in title project</summary><DocCode label="examples/editor-demo.celesta.json" language="json" code={timelineExample.trim()} /></details>
      <p><code>examples/minimal.celesta.json</code> is an empty starting point. For a visible result, begin with the title project above, then change the text and save. Use <strong>File → Reload</strong> to update the preview.</p>
      <h3>Animate with keyframes</h3><p>In a JSON transform, <code>position</code> places the item’s center. Any transform value, <code>opacity</code>, and audio <code>volume</code> can be a fixed number or keyframes. Keyframe times are relative to the item’s start, and <code>easing</code> shapes the curve into that keyframe:</p>
      <DocCode label="Fade an item in over half a second" language="json" code={'"opacity": {\n  "type": "keyframes",\n  "keyframes": [\n    { "time": { "value": 0, "timescale": 30 }, "value": 0 },\n    { "time": { "value": 15, "timescale": 30 }, "value": 1, "easing": "ease-out" }\n  ]\n}'} />
      <p>Easing names match the React <code>Easings</code> in kebab case, such as <code>ease-in-out-cubic</code> or <code>ease-out-back</code>.</p>
      <h3>Combine a project with React</h3><p>A React composition can include <code>{'<ProjectTimeline />'}</code>. Pass its companion JSON project when exporting so Celesta can resolve the timeline:</p>
      <details className="doc-details"><summary>View the companion React composition</summary><DocCode label="with-project.tsx" language="tsx" code={withProject.trim()} /></details><p>Save this as <code>with-project.tsx</code> beside your JSON file, then use the combined-composition command in <a href="#export">Export a video</a>. To draw a single track instead of the whole timeline, use <code>{'<ProjectTrack id="…" />'}</code>.</p>
    </>,
  },
  {
    id: 'dialogue', title: 'Character dialogue', description: 'Bring portraits, subtitles, and voices together.', keywords: 'dialogue character CharacterView portrait expressions subtitles voice lip sync lipsync mouth psd pfv PSDTool voiceroid talk conversation', content: <>
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
  },
  {
    id: 'data', title: 'Data, properties & components', description: 'Load data once, expose settings, and reuse components from JSON.', keywords: 'prepare async fetch data defineProjectProperties useProjectProperty ProjectProvider loadProject registerComponent component inspector schema', content: <>
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
  },
  {
    id: 'export', title: 'Export a video', description: 'Take your composition from the preview to an MP4.', keywords: 'mp4 H264 AAC render cli command line from to overwrite export', content: <>
      <h3>From the app</h3><p>Choose <strong>Export…</strong> and select an MP4 destination. For a section of the composition, press <kbd>I</kbd> and <kbd>O</kbd> to mark the start and end. The status bar shows progress; <strong>Cancel export</strong> stops the job.</p>
      <h3>From the command line</h3><ExportCommands />
      <p>Time values accept seconds, <code>MM:SS.mmm</code>, or <code>HH:MM:SS.mmm</code>. The selected span becomes a new video starting at its own 00:00.</p>
      <Note title="Before you render">MP4 output uses H.264 video and AAC audio. Width and height must be non-zero, even numbers. Keep every referenced local media file available during export.</Note>
    </>,
  },
  {
    id: 'reference', title: 'React essentials', description: 'A compact reference for everything in @celesta/react.', keywords: 'api props Composition Rect Text Group Image Video Audio Font Sequence Transition Character CharacterView Dialogue hooks reference', content: <>
      <p>Import these APIs from <code>@celesta/react</code>. Celesta’s TypeScript declarations provide the complete prop types and editor completions.</p>
      <Api caption="Layers" rows={[
        [<code>Composition</code>, <>Set <code>width</code>, <code>height</code>, <code>fps</code>, and <code>durationInFrames</code>. Return exactly one from your default export.</>],
        [<code>Rect</code>, <>A rectangle with <code>width</code>, <code>height</code>, and optional <code>fill</code>, <code>stroke</code>, <code>strokeWidth</code>, and <code>cornerRadius</code>.</>],
        [<code>Text</code>, <>Text from string or number children, styled with <code>style</code> and wrapped with <code>maxWidth</code>. See <a href="#layout">Shapes, text & layout</a>.</>],
        [<code>Group</code>, <>Applies a shared position, scale, rotation, and opacity to child layers.</>],
        [<><code>Image</code> / <code>Video</code> / <code>Audio</code></>, <>Local media through <code>src</code>. See <a href="#media">Images, video & sound</a>.</>],
        [<code>Font</code>, <>Loads a local font file through <code>src</code> so <code>Text</code> can use its family. See <a href="#layout">Shapes, text & layout</a>.</>],
      ]} />
      <Api caption="Time and motion" rows={[
        [<code>Sequence</code>, <>Places children at a frame offset with <code>from</code> and an optional <code>durationInFrames</code>.</>],
        [<code>Transition</code>, <>A fade, slide, or scale at the start or end of the enclosing sequence.</>],
        [<><code>interpolate</code> / <code>Easings</code></>, <>Map a frame to a value, with easing and extrapolation.</>],
        [<code>spring</code>, <>A physics-based value that settles from 0 to 1.</>],
        [<><code>useCurrentFrame()</code> / <code>useCurrentTime()</code></>, <>The current frame, or the exact time, local to an enclosing sequence.</>],
        [<code>useVideoConfig()</code>, <>The composition’s (or sequence’s) dimensions, frame rate, and duration.</>],
        [<code>timecodeToFrame()</code>, <>Converts a <code>MM:SS.mmm</code>-style timecode to a frame number.</>],
      ]} />
      <Api caption="Characters" rows={[
        [<><code>Assets</code> / <code>Character</code></>, <>Declare a character’s portrait, expressions, lip-sync mouths, and subtitle style.</>],
        [<code>CharacterView</code>, <>Places a character’s portrait. Accepts <code>expression</code>, <code>mouth</code>, and <code>lipSync</code>.</>],
        [<code>Dialogue</code>, <>Shows a line as a subtitle, plays its <code>audio</code>, and changes the view’s expression while active.</>],
        [<><code>loadLipSync</code> / <code>useLipSync</code></>, <>Generate mouth shapes from a WAV recording, and read the current shape.</>],
        [<code>loadPsdPreset</code>, <>Load visible PSD layers from a PSDTool <code>.pfv</code> favorite.</>],
      ]} />
      <Api caption="Layout, data, and tools" rows={[
        [<><code>Center</code>, <code>SafeArea</code>, <code>Stack</code>, <code>Grid</code>, <code>Fit</code></>, <>Arrange child layers. See <a href="#layout">layout helpers</a>.</>],
        [<><code>preloadMedia</code> / <code>mediaDurationInFrames</code></>, <>Read a media file’s length and size in <code>prepare()</code>.</>],
        [<><code>loadProject</code>, <code>ProjectProvider</code>, <code>useProjectProperty</code></>, <>Read values from a JSON project.</>],
        [<><code>ProjectTimeline</code> / <code>ProjectTrack</code></>, <>Draw a companion JSON project’s timeline, or one of its tracks.</>],
        [<><code>registerComponent</code> / <code>defineProjectProperties</code></>, <>Make components and properties available to JSON projects and the Inspector.</>],
        [<><code>DebugOverlay</code> / <code>DebugBounds</code> / <code>useIsPreview()</code></>, <>Preview-only guides that never appear in an export.</>],
      ]} />
    </>,
  },
  {
    id: 'examples', title: 'Examples to build on', description: 'Start with something small. Make it yours.', keywords: 'examples samples inspiration title layout animation dialogue project properties', content: <>
      <div className="doc-example-grid">
        <a href="#react-compositions"><span>01 / REACT</span><strong>A title in motion</strong><p>Follow the complete fading-title example in this guide.</p></a>
        <a href="#timelines"><span>02 / JSON</span><strong>A first timeline</strong><p>Read the built-in overlay project and adapt its text.</p></a>
        <a href="#dialogue"><span>03 / DIALOGUE</span><strong>Give it a voice</strong><p>Pair a portrait, subtitles, and a voice with lip sync.</p></a>
      </div>
      <h3>Included with the app</h3><p>Copy an example into your own project folder before editing it.</p><ul><li><strong>macOS:</strong> in Finder, right-click Celesta in Applications, choose <strong>Show Package Contents</strong>, then open <code>Contents/Resources/examples</code>.</li><li><strong>Windows:</strong> open the <code>examples</code> folder beside <code>Celesta.exe</code> in the installed or portable app folder.</li></ul>
      <p>The repository’s <code>packages/react/examples</code> directory contains more scenes covering text, animation, layout, dialogue, lip sync, and editable project properties. Download the source archive, extract it, and open a supported entry through <strong>File → Open…</strong> in the installed app; no source build is needed.</p>
      <a className="doc-text-link" href={`${repository}/tree/main/packages/react/examples`}>Browse all source examples on GitHub ↗</a>
      <p>You can also <a href="/#playground">visit the website playground</a>, customize its title and palette, and download the <code>.tsx</code> composition it previews. The browser draws that same file, so the download looks the same when you open it in Celesta.</p>
    </>,
  },
  {
    id: 'build-from-source', title: 'Build from source', description: 'For contributors, custom builds, and Linux users.', keywords: 'developer source code clone cargo Rust FFmpeg Node pnpm codegen macOS Windows Linux', content: <SourceBuild />,
  },
  {
    id: 'troubleshooting', title: 'Troubleshooting', description: 'A few things to check when your scene needs a hand.', keywords: 'errors missing media build FFmpeg npm typescript black blank reload limitations help lip sync font subtitle', content: <>
      <details className="doc-details" open><summary>The app cannot find its bundled runtime</summary><p>On macOS, copy the complete app into Applications before launching it. For a portable Windows build, extract the entire archive and keep <code>runtime</code> and the DLLs beside <code>Celesta.exe</code>. If files are missing, extract or install the package again.</p></details>
      <details className="doc-details"><summary>A source build cannot find FFmpeg</summary><p>Confirm that the 8.1.x development libraries are installed, not just the executable. On macOS, set <code>PKG_CONFIG_PATH</code> in the same shell where you run Cargo. On Windows, check <code>VCPKG_ROOT</code> and the MSVC build tools. See <a href="#build-from-source">source-build setup</a>.</p></details>
      <details className="doc-details"><summary>My media is missing</summary><p>Celesta currently supports local media files, not remote media URLs. Check the paths in your project, keep the referenced files alongside the project when sharing it, and look for missing-asset indicators in the Assets panel.</p></details>
      <details className="doc-details"><summary>The preview did not change after saving</summary><p>React compositions reload automatically. For JSON projects, use <strong>File → Reload</strong>. Make sure you saved the file you actually opened in Celesta; the app itself does not edit the source.</p></details>
      <details className="doc-details"><summary>My text uses the wrong font</summary><p><code>fontFamily</code> must match the family name of a font installed on the computer that renders the video, or of a font file loaded with <code>{'<Font>'}</code>. When it does not, Celesta falls back to another font. Check the exact family name in your system’s font manager. To avoid installing the font on every machine that exports the project, keep the font file next to the entry and load it with <code>{'<Font>'}</code>.</p></details>
      <details className="doc-details"><summary>The mouth does not move</summary><p>Lip sync needs an uncompressed WAV voice and a transcript with readable vowels (kana or romaji). Check that <code>loadLipSync()</code> runs in <code>prepare()</code>, that the track is passed as <code>lipSync</code>, and that each mouth path matches a layer in the PSD exactly. For a PSD that shows nothing, set <code>layers</code> from a PSDTool preset.</p></details>
      <details className="doc-details"><summary>My editor cannot resolve @celesta/react</summary><p>Open the composition in Celesta and choose <strong>File → Set Up TypeScript</strong>. Check that your <code>tsconfig.json</code> extends <code>./.celesta/tsconfig.json</code>. When building from source, also complete the React package’s install, codegen, and build steps.</p></details>
      <details className="doc-details"><summary>MP4 export will not start</summary><p>Use non-zero, even-numbered dimensions and confirm that all local media is available. If the destination file already exists, choose another path or explicitly add <code>--overwrite</code> to the CLI command.</p></details>
      <h3>Still stuck?</h3><p>Open an issue with your operating system, reproduction steps, and the exact error message. Include a small project and media you have permission to share when possible.</p>
      <a className="doc-text-link" href={`${repository}/issues`}>Report a problem on GitHub ↗</a>
    </>,
  },
];
