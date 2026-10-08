import { docPath } from './docs-nav';
import { currentLocale, t, text, homePath } from './i18n';
import { showcasePath } from './showcase-catalog';
import { useState, type ReactNode } from 'react';
import { DocCode } from './DocCode';
import { Api, Note } from './docs-shared';
import { allDownloads, downloads, repository } from './links';
import firstScene from './examples/first-scene.tsx?raw';
import dialogueScene from './examples/dialogue.tsx?raw';
import dialogueProject from './examples/dialogue.celesta.json?raw';
import dialogueSeriesScene from './examples/dialogue-series.tsx?raw';
import transitionSeriesScene from './examples/transition-series.tsx?raw';
import fittedTextScene from './examples/fitted-text.tsx?raw';
import dataScene from './examples/data.tsx?raw';
import lipSyncScene from './examples/lip-sync.tsx?raw';
import mediaScene from './examples/media.tsx?raw';
import timelineExample from '../../../examples/editor-demo.celesta.json?raw';
import withProject from '../../react/examples/with-project.tsx?raw';

export { repository };

function SourceBuild() {
  const [platform, setPlatform] = useState('macOS');
  return <>
    <p>{text('docs.chapters.build-from-source.to-build-celesta-install-rust-1-89', [<strong />, <strong />])}</p>
    <Note title={t('docs.chapters.build-from-source.the-libraries-matter')}>{text('docs.chapters.build-from-source.the-ffmpeg-command-line-executable-alone-is')}</Note>
    <h3>{text('docs.chapters.build-from-source.1-prepare-your-platform')}</h3>
    <div className="platform-options" role="group" aria-label={t('docs.chapters.build-from-source.source-build-platform')}>{['macOS', 'Windows', 'Linux'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    <div className="platform-guide">
      {platform === 'macOS' && <><h4>{text('docs.chapters.build-from-source.macos')}</h4><p>{text('docs.chapters.build-from-source.install-the-xcode-command-line-tools-and', [<code>pkg-config</code>])}</p><DocCode label={t('docs.chapters.build-from-source.macos-dependencies')} code={'xcode-select --install\nbrew install ffmpeg@8 pkg-config\nexport PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"'} /></>}
      {platform === 'Windows' && <><h4>{text('docs.chapters.build-from-source.windows')}</h4><p>{text('docs.chapters.build-from-source.install-the-msvc-c-build-tools-and', [<code>VCPKG_ROOT</code>])}</p><DocCode label={t('docs.chapters.build-from-source.windows-dependencies')} code={'vcpkg install ffmpeg[x264]:x64-windows-static-md'} /><p>{text('docs.chapters.build-from-source.your-vcpkg-checkout-must-provide-the-supported')}</p></>}
      {platform === 'Linux' && <><h4>{text('docs.chapters.build-from-source.linux')}</h4><p>{text('docs.chapters.build-from-source.most-distributions-including-ubuntu-24-04-lts', [<code>scripts/build-ffmpeg-linux.sh</code>, <code>libx264</code>, <code>/opt/ffmpeg8</code>, <code>libclang-dev</code>])}</p><DocCode label={t('docs.chapters.build-from-source.linux-dependencies')} code={'sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libclang-dev\nsudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev\nsudo scripts/build-ffmpeg-linux.sh /opt/ffmpeg8\nexport PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig'} /><p>{text('docs.chapters.build-from-source.set-in-every-shell-where-you-run', [<code>PKG_CONFIG_PATH</code>, <code>LD_LIBRARY_PATH</code>, <code>pkg-config</code>, <code>libavcodec</code>, <code>libavformat</code>, <code>libavfilter</code>, <code>libavdevice</code>, <code>libavutil</code>, <code>libswscale</code>, <code>libswresample</code>, <a href={docPath('export')} />])}</p></>}
    </div>
    <h3>{text('docs.chapters.build-from-source.2-get-the-source')}</h3>
    <DocCode label={t('docs.chapters.build-from-source.clone-celesta')} code={`git clone ${repository}.git celesta\ncd celesta`} />
    <h3>{text('docs.chapters.build-from-source.build-runtime')}</h3><p>{text('docs.chapters.build-from-source.for-react-compositions-in-a-source-build')}</p>
    <DocCode label={t('docs.chapters.build-from-source.build-the-react-runtime-from-source')} code={'pnpm install\npnpm --dir packages/react run codegen\npnpm --dir packages/react run build'} />
    <h3>{text('docs.chapters.build-from-source.open-composition')}</h3>
    <DocCode label={t('docs.chapters.build-from-source.launch-the-native-app')} code="cargo run -p celesta-editor --release -- packages/react/examples/title.tsx" />
    <p>{text('docs.chapters.build-from-source.celesta-opens-its-built-in-demo-you')}</p>
    <DocCode label={t('docs.chapters.build-from-source.open-the-example-project')} code="cargo run -p celesta-editor --release -- examples/reel/film.tsx" />
    <p>{text('docs.chapters.build-from-source.run-these-commands-from-the-repository-root', [<code>examples/</code>])}</p>
    <DocCode label={t('docs.chapters.build-from-source.initialize-a-project-from-source')} code="cargo run -p celesta-editor --release -- --init my-video" />
    <p>{text('docs.chapters.build-from-source.see-create-a-project-for-the-generated', [<a href={docPath('create-project')} />])}</p>
    <DocCode label={t('docs.chapters.build-from-source.preview-react-from-source')} code="cargo run -p celesta-editor --release -- packages/react/examples/title.tsx" />
    <DocCode label={t('docs.chapters.build-from-source.export-react-from-source')} code="cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx output.mp4" />
    <p>{text('docs.chapters.build-from-source.run-cargo-commands-from-the-repository-root', [<a href={docPath('react-compositions')} />])}</p>
  </>;
}


function Installation() {
  const [platform, setPlatform] = useState('macOS');
  return <>
    <p>{text('docs.chapters.installation.download-the-celesta-package-for-your-operating')}</p>
    <div className="doc-downloads">{text('docs.chapters.installation.download-for-macos-download-for-windows', [<a className="button button-primary doc-download" href={downloads.macOS} />, <span aria-hidden="true" />, <a className="button button-secondary" href={downloads.Windows} />, <span aria-hidden="true" />])}</div>
    <p className="doc-small">{text('docs.chapters.installation.looking-for-another-version-or-architecture-browse', [<a href={allDownloads} />])}</p>
    <Note title={t('docs.chapters.installation.everything-you-need-to-run-celesta')}>{text('docs.chapters.installation.the-macos-and-windows-packages-include-the')}</Note>
    <h3>{text('docs.chapters.installation.1-install-the-app')}</h3>
    <div className="platform-options" role="group" aria-label={t('docs.chapters.installation.installation-platform')}>{['macOS', 'Windows', 'Linux'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    <div className="platform-guide">
      {platform === 'macOS' && <><h4>{text('docs.chapters.installation.macos')}</h4><ol><li>{text('docs.chapters.installation.choose-the-package-for-apple-silicon-or', [<code>macos-arm64.dmg</code>, <code>macos-x64.dmg</code>])}</li><li>{text('docs.chapters.installation.open-the-disk-image-and-drag-celesta', [<strong />, <strong />])}</li><li>{text('docs.chapters.installation.eject-the-disk-image-then-open-celesta')}</li></ol></>}
      {platform === 'Windows' && <><h4>{text('docs.chapters.installation.windows-x64')}</h4><p>{text('docs.chapters.installation.download-the-installer-run-it-and-open', [<code>windows-x64-setup.exe</code>])}</p><p>{text('docs.chapters.installation.prefer-a-portable-app-download-the-package', [<code>windows-x64.zip</code>, <code>Celesta.exe</code>])}</p></>}
      {platform === 'Linux' && <><h4>{text('docs.chapters.installation.linux')}</h4><p>{text('docs.chapters.installation.use-the-source-build-instructions-for-linux', [<a href={docPath('build-from-source')} />])}</p></>}
    </div>
    <h3>{text('docs.chapters.installation.2-create-your-project')}</h3><p>{text('docs.chapters.installation.choose-file-create-new-project-or-the', [<strong />, <strong />, <code>Documents/my-video</code>, <code>film.tsx</code>, <a href={docPath('create-project')} />])}</p>
    <h3>{text('docs.chapters.installation.3-edit-your-first-scene')}</h3><p>{text('docs.chapters.installation.open-in-your-text-editor-change-the', [<code>film.tsx</code>, <kbd>Space</kbd>, <a href={docPath('react-compositions')} />, <code>first-scene.tsx</code>, <strong />])}</p>
    <p>{text('docs.chapters.installation.prefer-an-existing-example-the-app-package', [<code>title.tsx</code>, <a href={docPath('examples')} />])}</p>
    <h3>{text('docs.chapters.installation.updating-celesta')}</h3><p>{text('docs.chapters.installation.quit-celesta-and-install-the-newer-release')}</p>
    <p>{text('docs.chapters.installation.working-on-celesta-itself-see-build-from', [<a href={docPath('build-from-source')} />])}</p>
  </>;
}

function CreateProject() {
  const [platform, setPlatform] = useState('macOS');
  const executable = platform === 'macOS'
    ? '"/Applications/Celesta.app/Contents/MacOS/Celesta"'
    : '& "C:\\path\\to\\Celesta\\Celesta.exe"';
  return <>
    <p>{text('docs.chapters.create-project.initialize-a-folder-for-your-react-composition')}</p>
    <h3>{text('docs.chapters.create-project.from-the-app')}</h3>
    <p>{text('docs.chapters.create-project.choose-file-create-new-project-click-create', [<strong />, <strong />, <kbd>⌘ N</kbd>, <kbd>Ctrl N</kbd>, <code>film.tsx</code>])}</p>
    <h3>{text('docs.chapters.create-project.from-the-command-line')}</h3>
    <p>{text('docs.chapters.create-project.with-on-path-pass-the-folder-to', [<code>celesta-editor</code>, <code>--init</code>])}</p>
    <DocCode label={t('docs.chapters.create-project.initialize-and-open-a-project')} code={'celesta-editor --init my-video\ncd my-video\ncelesta-editor film.tsx\ncelesta-exporter --react film.tsx output.mp4'} />
    <DocCode label={t('docs.chapters.create-project.initialize-the-current-directory')} code="celesta-editor --init" />
    <p>{text('docs.chapters.create-project.desktop-packages-name-the-executables-and-on', [<code>Celesta</code>, <code>Celesta-export</code>, <code>.exe</code>, <code>celesta-editor</code>, <code>celesta-exporter</code>])}</p>
    <div className="platform-options" role="group" aria-label={t('docs.chapters.create-project.project-initialization-platform')}>{['macOS', 'Windows'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    {platform === 'Windows' ? <p>{text('docs.chapters.create-project.use-powershell-and-replace-with-your-installed', [<code>C:\path\to\Celesta</code>])}</p> : <p>{text('docs.chapters.create-project.this-command-assumes-the-app-is-installed', [<code>/Applications</code>])}</p>}
    <DocCode label={t('docs.chapters.create-project.initialize-with-the-installed-app')} code={`${executable} --init my-video`} />
    <p>{text('docs.chapters.create-project.for-a-source-build-build-the-react', [<a href={docPath('build-from-source')} />, <code>cargo run -p celesta-editor --release -- --init my-video</code>])}</p>
    <h3>{text('docs.chapters.create-project.what-initialization-creates')}</h3>
    <ul>
      <li>{text('docs.chapters.create-project.a-five-second-1920-1080-composition-at', [<code>film.tsx</code>])}</li>
      <li>{text('docs.chapters.create-project.a-private-project-with-preview-export-and', [<code>package.json</code>, <code>^7.0.2</code>])}</li>
      <li>{text('docs.chapters.create-project.and-configuration-and-declarations-matching-celesta-s', [<code>tsconfig.json</code>, <code>.celesta/</code>])}</li>
      <li>{text('docs.chapters.create-project.ignores-and-the-generated-directory-ignores-itself', [<code>.gitignore</code>, <code>node_modules/</code>, <code>output.mp4</code>, <code>.celesta/</code>])}</li>
    </ul>
    <p>{text('docs.chapters.create-project.preview-and-export-work-without-installing-packages')}</p>
    <DocCode label={t('docs.chapters.create-project.install-the-typescript-compiler-and-check-your')} code={'pnpm install\npnpm typecheck'} />
    <Note title={t('docs.chapters.create-project.initializing-an-existing-folder')}>{text('docs.chapters.create-project.re-running-initialization-refreshes-and-creates-missing', [<code>.celesta/</code>, <code>package.json</code>, <code>tsconfig.json</code>, <code>.gitignore</code>, <code>./.celesta/tsconfig.json</code>])}</Note>
    <h3>{text('docs.chapters.create-project.add-npm-dependencies')}</h3>
    <p>{text('docs.chapters.create-project.install-external-packages-in-the-project-that')}</p>
    <DocCode label={t('docs.chapters.create-project.add-a-project-dependency')} code="pnpm add ag-psd" />
    <p>{text('docs.chapters.create-project.preview-and-export-resolve-external-imports-from', [<code>node_modules</code>, <code>@celesta/react</code>, <code>@celesta/math</code>, <code>@celesta/code</code>, <code>@celesta/voicevox</code>])}</p>
    <p>{text('docs.chapters.create-project.node-asset-preparation-scripts-use-the-same', [<code>prepare-assets.mjs</code>, <code>package.json</code>, <code>portrait.psd</code>])}</p>
    <DocCode label="prepare-assets.mjs" language="tsx" code={"import { readFileSync, writeFileSync } from 'node:fs';\nimport { readPsd } from 'ag-psd';\n\nconst { width, height } = readPsd(\n  readFileSync(new URL('./portrait.psd', import.meta.url)),\n  { skipLayerImageData: true, skipCompositeImageData: true, skipThumbnail: true },\n);\nwriteFileSync(\n  new URL('./portrait-info.json', import.meta.url),\n  JSON.stringify({ width, height }),\n);"} />
    <DocCode label={t('docs.chapters.create-project.prepare-assets')} code="node prepare-assets.mjs" />
    <p>{text('docs.chapters.create-project.import-the-generated-json-in-your-composition', [<code>.ts</code>])}</p>
    <h3>{text('docs.chapters.create-project.dependencies-in-repository-examples')}</h3>
    <p>{text('docs.chapters.create-project.the-react-examples-under-belong-to-the', [<code>examples/</code>, <code>pnpm install</code>])}</p>
    <DocCode label={t('docs.chapters.create-project.add-a-dependency-to-an-example-and')} code={'pnpm --dir examples/versus add ag-psd\ncargo run -p celesta-editor --release -- examples/versus/film.tsx\ncargo run -p celesta-exporter --release -- --react examples/versus/film.tsx output.mp4'} />
    <p>{text('docs.chapters.create-project.example-manifests-use-for-local-celesta-packages', [<code>workspace:*</code>, <a href={docPath('build-from-source')} />])}</p>
    <p>{text('docs.chapters.create-project.continue-with-your-first-react-composition-preview', [<a href={docPath('react-compositions')} />, <a href={docPath('preview')} />, <a href={docPath('export')} />])}</p>
  </>;
}

function ExportCommands() {
  const [platform, setPlatform] = useState('macOS');
  const executable = platform === 'macOS'
    ? '"/Applications/Celesta.app/Contents/MacOS/Celesta-export"'
    : '& "C:\\path\\to\\Celesta\\Celesta-export.exe"';
  return <>
    <p>{text('docs.shared.the-app-includes-a-command-line-exporter', [<code>first-scene.tsx</code>])}</p>
    <div className="platform-options" role="group" aria-label={t('docs.shared.exporter-platform')}>{['macOS', 'Windows'].map(os => <button key={os} onClick={() => setPlatform(os)} aria-pressed={platform === os}>{os}</button>)}</div>
    {platform === 'Windows' ? <p>{text('docs.shared.use-powershell-replace-with-your-installed-or', [<code>C:\path\to\Celesta</code>])}</p> : <p>{text('docs.shared.these-commands-assume-celesta-is-installed-in', [<code>/Applications</code>])}</p>}
    <DocCode label={t('docs.shared.export-a-react-composition')} code={`${executable} --react first-scene.tsx output.mp4`} />
    <p>{text('docs.shared.add-to-replace-an-existing-output-file', [<code>--overwrite</code>, <code>--from</code>, <code>--to</code>])}</p>
    <DocCode label={t('docs.shared.export-the-first-second')} code={`${executable} --from 0 --to 1 --react first-scene.tsx section.mp4`} />
    <h3>{text('docs.shared.encoding-options')}</h3>
    <p>{text('docs.shared.encoding-options-body')}</p>
    <DocCode label={t('docs.shared.export-a-draft')} code={`${executable} --react --preset veryfast --crf 18 first-scene.tsx draft.mp4`} />
    <h3>{text('docs.shared.png-frames')}</h3>
    <p>{text('docs.shared.png-frames-body')}</p>
    <DocCode label={t('docs.shared.export-selected-frames')} code={`${executable} --react --frames 0,90 first-scene.tsx check.png`} />
    <DocCode label={t('docs.shared.export-contact-sheet')} code={`${executable} --react --every 30 --contact-sheet --columns 4 --tile-width 320 first-scene.tsx sheet.png`} />
    <h3>{text('docs.shared.graphics-driver')}</h3>
    <p>{text('docs.shared.graphics-driver-body')}</p>
    <DocCode label={t('docs.shared.select-graphics-driver')} code={`${executable} --react --driver auto first-scene.tsx output.mp4`} />
    <p>{text('docs.shared.for-scripts-and-ai-agents-add-the', [<code>--json</code>])}</p>
    <p>{text('docs.shared.using-a-source-build-the-developer-instructions', [<a href={docPath('build-from-source')} />])}</p>
  </>;
}

export const contents: Record<string, ReactNode> = {
  'installation': <Installation />,
  'create-project': <CreateProject />,
  'preview': <>
      <p>{text('docs.chapters.preview.choose-file-open-to-open-a-project', [<strong />, <code>.tsx</code>, <code>.jsx</code>, <code>.ts</code>, <code>.js</code>])}</p>
      <Note title={t('docs.chapters.preview.your-source-is-your-canvas')}>{text('docs.chapters.preview.edit-projects-and-compositions-in-your-text')}</Note>
      <p>{text('docs.chapters.preview.react-compositions-reload-when-you-save-for', [<strong />])}</p>
      <div className="doc-table-wrap" tabIndex={0} aria-label={t('docs.chapters.preview.keyboard-shortcuts')}><table><caption>{text('docs.chapters.preview.keyboard-shortcuts-label')}</caption><thead><tr><th>{text('docs.chapters.preview.action')}</th><th>{text('docs.chapters.preview.macos')}</th><th>{text('docs.chapters.preview.windows-linux')}</th></tr></thead><tbody>
        <tr><td>{text('docs.chapters.preview.create-a-new-project')}</td><td><kbd>⌘ N</kbd></td><td><kbd>Ctrl N</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.open-a-project')}</td><td><kbd>⌘ O</kbd></td><td><kbd>Ctrl O</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.reload-a-json-project')}</td><td><kbd>⌘ R</kbd></td><td><kbd>Ctrl R</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.play-pause')}</td><td colSpan={2}><kbd>Space</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.step-backward-forward')}</td><td colSpan={2}><kbd>←</kbd> / <kbd>→</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.jump-one-second')}</td><td colSpan={2}><kbd>Shift ←</kbd> / <kbd>Shift →</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.jump-clip-edge')}</td><td colSpan={2}><kbd>↑</kbd> / <kbd>↓</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.jump-start-end')}</td><td colSpan={2}><kbd>Home</kbd> / <kbd>End</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.loop-playback')}</td><td><kbd>⌘ /</kbd></td><td><kbd>Ctrl /</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.show-safe-areas')}</td><td colSpan={2}><kbd>'</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.zoom-timeline')}</td><td colSpan={2}><kbd>=</kbd> / <kbd>-</kbd> / <kbd>Shift Z</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.clear-export-range')}</td><td colSpan={2}><kbd>Shift X</kbd></td></tr>
        <tr><td>{text('docs.chapters.preview.mark-export-start-end')}</td><td colSpan={2}><kbd>I</kbd> / <kbd>O</kbd></td></tr>
      </tbody></table></div>
      <h3>{text('docs.chapters.preview.listen-as-you-look')}</h3><p>{text('docs.chapters.preview.playback-synchronizes-the-picture-and-audio-use')}</p>
      <h3>{text('docs.chapters.preview.preview-only-guides')}</h3><p>{text('docs.chapters.preview.draws-the-frame-edge-a-safe-area', [<code>{'<DebugOverlay />'}</code>, <code>{'<DebugBounds>'}</code>, <code>useIsPreview()</code>])}</p>
    </>,
  'react-compositions': <>
      <p>{text('docs.chapters.react-compositions.the-installed-app-already-includes-the-react', [<a href={docPath('create-project')} />, <code>film.tsx</code>])}</p>
      <h3>{text('docs.chapters.react-compositions.make-your-first-scene')}</h3><p>{text('docs.chapters.react-compositions.create-in-your-own-project-folder-this', [<code>first-scene.tsx</code>])}</p>
      <DocCode label="first-scene.tsx" language="tsx" code={firstScene.trim()} />
      <p>{text('docs.chapters.react-compositions.in-celesta-choose-file-open-and-select', [<strong />, <code>first-scene.tsx</code>, <kbd>Space</kbd>])}</p>
      <p>{text('docs.chapters.react-compositions.change-the-text-or-colors-and-save', [<code>Text</code>, <code>Rect</code>, <code>Group</code>])}</p>
      <h3>{text('docs.chapters.react-compositions.how-a-composition-is-put-together')}</h3>
      <ul>
        <li>{text('docs.chapters.react-compositions.the-file-s-default-export-returns-a', [<strong />, <code>{'<Composition>'}</code>])}</li>
        <li>{text('docs.chapters.react-compositions.everything-inside-it-is-drawn-in-order')}</li>
        <li>{text('docs.chapters.react-compositions.your-own-components-are-ordinary-react-function', [<code>.map()</code>])}</li>
        <li>{text('docs.chapters.react-compositions.an-optional-named-export-runs-once-before', [<code>prepare()</code>, <a href={docPath('data')} />])}</li>
      </ul>
      <h3>{text('docs.chapters.react-compositions.set-up-typescript-in-your-editor')}</h3><p>{text('docs.chapters.react-compositions.with-a-react-composition-open-choose-file', [<strong />, <code>@celesta/react</code>, <code>@celesta/math</code>, <code>@celesta/code</code>, <code>.celesta/</code>, <code>@celesta/voicevox</code>])}</p>
      <p>{text('docs.chapters.react-compositions.starting-a-new-project-with-no-composition', [<strong />])}</p>
      <p>{text('docs.chapters.react-compositions.if-there-is-no-celesta-creates-one', [<code>tsconfig.json</code>, <code>extends</code>])}</p>
      <DocCode label="tsconfig.json" language="json" code={'{\n  "extends": "./.celesta/tsconfig.json"\n}'} />
      <p>{text('docs.chapters.react-compositions.you-do-not-need-to-install-those', [<code>.celesta/</code>])}</p>
    </>,
  'animation': <>
      <p>{text('docs.chapters.animation.a-composition-s-duration-in-seconds-is', [<code>durationInFrames / fps</code>, <code>useCurrentFrame()</code>])}</p>
      <h3>{text('docs.chapters.animation.interpolate-between-values')}</h3><p>{text('docs.chapters.animation.the-title-example-uses-for-opacity-for', [<code>Math.min(frame / 30, 1)</code>, <code>interpolate</code>, <code>Easings</code>, <code>@celesta/react</code>])}</p>
      <DocCode label={t('docs.chapters.animation.an-eased-fade-inside-a-component')} language="tsx" code={"const opacity = interpolate(frame, [0, 30], [0, 1], {\n  easing: Easings.easeOut,\n  extrapolateLeft: 'clamp',\n  extrapolateRight: 'clamp',\n});"} />
      <p>{text('docs.chapters.animation.pass-the-result-to-input-ranges-must', [<code>{'<Text opacity={opacity} … />'}</code>, <code>extend</code>, <code>clamp</code>])}</p>
      <p>{text('docs.chapters.animation.covers-the-familiar-easings-net-curves-and', [<code>Easings</code>, <code>linear</code>, <code>easeIn</code>, <code>easeOut</code>, <code>easeInOut</code>, <code>Sine</code>, <code>Quad</code>, <code>Cubic</code>, <code>Quart</code>, <code>Quint</code>, <code>Expo</code>, <code>Circ</code>, <code>Back</code>, <code>Elastic</code>, <code>Bounce</code>, <code>Easings.easeOutBack</code>])}</p>
      <h3>{text('docs.chapters.animation.interpolate-between-colors')}</h3><p>{text('docs.chapters.animation.import-from-too-it-takes-the-same', [<code>interpolateColor</code>, <code>@celesta/react</code>, <code>easing</code>, <code>interpolate</code>, <code>#RRGGBB</code>, <code>#RRGGBBAA</code>, <code>#RRGGBBAA</code>, <code>Rect</code>])}</p>
      <DocCode label={t('docs.chapters.animation.night-to-day-and-a-title-fading')} language="tsx" code={"const sky = interpolateColor(frame, [0, 45, 90], ['#101820', '#EF7B45', '#7FC8F8']);\nconst title = interpolateColor(frame, [0, 20], ['#FFFFFF00', '#FFFFFF']);\n\n<Rect width={1920} height={1080} fill={sky} />\n<Text style={{ fontSize: 96, fill: { type: 'solid', color: title } }}>Morning</Text>"} />
      <p>{text('docs.chapters.animation.unlike-extrapolation-defaults-to-so-the-first', [<code>interpolate</code>, <code>clamp</code>, <code>extend</code>, <code>identity</code>, <code>#00000000</code>, <code>extend</code>, <code>rgb()</code>, <code>#RGB</code>])}</p>
      <h3>{text('docs.chapters.animation.add-a-little-bounce')}</h3><p>{text('docs.chapters.animation.simulates-a-damped-spring-it-starts-at', [<code>spring()</code>, <code>useVideoConfig()</code>])}</p>
      <DocCode label={t('docs.chapters.animation.a-springy-pop-in')} language="tsx" code={"function Badge() {\n  const frame = useCurrentFrame();\n  const { fps } = useVideoConfig();\n  const scale = spring({ frame, fps, delay: 10, config: { damping: 12 } });\n  return (\n    <Rect x={960} y={540} anchorX={0.5} anchorY={0.5} scale={scale}\n      width={320} height={120} cornerRadius={60} fill=\"#78618e\" />\n  );\n}"} />
      <p>{text('docs.chapters.animation.tune-the-feel-with-default-100-default', [<code>config</code>, <code>stiffness</code>, <code>damping</code>, <code>mass</code>, <code>overshootClamping</code>, <code>from</code>, <code>to</code>])}</p>
      <h3>{text('docs.chapters.animation.place-a-scene-on-the-timeline')}</h3><p>{text('docs.chapters.animation.import-to-delay-or-limit-a-group', [<code>Sequence</code>, <code>Title</code>])}</p>
      <DocCode label={t('docs.chapters.animation.inside-your-composition')} language="tsx" code={'<Sequence from={30} durationInFrames={90}>\n  <Title />\n</Sequence>'} />
      <p>{text('docs.chapters.animation.within-that-sequence-starts-at-0-when', [<code>useCurrentFrame()</code>])}</p>
      <h3>{text('docs.chapters.animation.redraw-a-past-frame')}</h3><p>{text('docs.chapters.animation.every-frame-is-a-function-of-the', [<code>{'<FreezeFrame frame>'}</code>, <code>frame</code>, <code>Sequence</code>])}</p>
      <DocCode label={t('docs.chapters.animation.frame-1200-at-quarter-size')} language="tsx" code={'<Group scale={0.25} x={80} y={80}>\n  <FreezeFrame frame={1200}>\n    <World />\n  </FreezeFrame>\n</Group>'} />
      <p>{text('docs.chapters.animation.directly-inside-returns-rounded-to-a-whole', [<code>useCurrentFrame()</code>, <code>frame</code>, <code>Sequence</code>, <code>frame</code>, <code>frame</code>, <code>useCurrentFrame()</code>, <code>{'frame={frame - 90}'}</code>, <code>CharacterView</code>, <code>Dialogue</code>, <code>FreezeFrame</code>, <code>ProjectTimeline</code>, <code>ProjectTrack</code>])}</p>
      <h3>{text('docs.chapters.animation.ready-made-entrances-and-exits')}</h3><p>{text('docs.chapters.animation.fades-slides-or-scales-its-children-at', [<code>{'<Transition>'}</code>, <code>direction="in"</code>, <code>direction="out"</code>])}</p>
      <DocCode label={t('docs.chapters.animation.slide-a-caption-in-fade-it-out')} language="tsx" code={'<Sequence from={30} durationInFrames={120}>\n  <Transition type="slide" slideFrom="bottom" durationInFrames={15}\n    easing={Easings.easeOutCubic}>\n    <Transition type="fade" direction="out" durationInFrames={20}>\n      <Caption />\n    </Transition>\n  </Transition>\n</Sequence>'} />
      <p>{text('docs.chapters.animation.slides-travel-pixels-64-by-default-scales', [<code>distance</code>, <code>scaleFrom</code>, <code>{"type={['fade', 'slide']}"}</code>])}</p>
      <Note title={t('docs.chapters.animation.keep-animation-tied-to-the-frame')}>{text('docs.chapters.animation.compute-visual-changes-from-frame-or-time', [<code>Math.random()</code>, <a href={docPath('math')} />, <code>@celesta/math</code>, <code>random</code>, <code>noise</code>])}</Note>
      <h3>{text('docs.chapters.animation.freeze-frame')}</h3><p>{text('docs.chapters.animation.freeze-frame-body', [<code>{'<FreezeFrame frame={60}>'}</code>])}</p>
      <DocCode label="FreezeFrame" language="tsx" code={'<FreezeFrame frame={60}>\n  <AnimatedTitle />\n</FreezeFrame>'} />
    </>,
  'motion-toolkit': <>
      <p>{text('docs.chapters.motion-toolkit.these-helpers-cover-the-patterns-that-come', [<code>Sequence</code>, <code>Group</code>, <a href={docPath('text-camera-lines')} />, <a href={docPath('math')} />, <code>@celesta/math</code>])}</p>
      <h3>{text('docs.chapters.motion-toolkit.scenes-back-to-back')}</h3><p>{text('docs.chapters.motion-toolkit.plays-its-children-one-after-another-so', [<code>{'<Series>'}</code>, <code>{'<Series.Sequence>'}</code>, <code>offset</code>, <code>computeSeries()</code>])}</p>
      <DocCode label={t('docs.chapters.motion-toolkit.three-scenes-sized-from-their-lengths')} language="tsx" code={"const SCENES = [\n  { name: 'intro', durationInFrames: 90, Scene: Intro },\n  { name: 'body', durationInFrames: 240, Scene: Body },\n  { name: 'outro', durationInFrames: 60, Scene: Outro },\n];\nconst { durationInFrames } = computeSeries(SCENES);\n\nexport default function Root() {\n  return (\n    <Composition width={1920} height={1080} fps={30} durationInFrames={durationInFrames}>\n      <Series>\n        {SCENES.map(({ name, durationInFrames, Scene }) => (\n          <Series.Sequence key={name} durationInFrames={durationInFrames}>\n            <Scene />\n          </Series.Sequence>\n        ))}\n      </Series>\n    </Composition>\n  );\n}"} />
      <h3>{text('docs.chapters.motion-toolkit.transitions-between-scenes')}</h3><p>{text('docs.chapters.motion-toolkit.transition-series-body', [<code>{'<TransitionSeries>'}</code>, <code>Series</code>, <code>{'<TransitionSeries.Transition>'}</code>, <code>'cut'</code>, <code>'crossfade'</code>, <code>'slide'</code>, <code>'wipe'</code>, <code>durationInFrames</code>, <code>computeTransitionSeries()</code>])}</p>
      <DocCode label={t('docs.chapters.motion-toolkit.scenes-joined-by-a-cross-fade-and-a-slide')} language="tsx" code={transitionSeriesScene.trim()} />
      <p>{text('docs.chapters.motion-toolkit.transition-series-rules', [<code>useTransitionVolume(volume)</code>, <code>useTransitionSeriesScene()</code>])}</p>
      <h3>{text('docs.chapters.motion-toolkit.cascades-and-entrances')}</h3><p>{text('docs.chapters.motion-toolkit.is-a-clamped-0-1-value-for', [<code>progress(frame, start, durationInFrames, easing?)</code>, <code>{'<Stagger each={n}>'}</code>, <code>n</code>])}</p>
      <DocCode label={t('docs.chapters.motion-toolkit.rows-that-arrive-one-after-another')} language="tsx" code={"function Row({ label, y }: { label: string; y: number }) {\n  const frame = useCurrentFrame(); // 0 when this row starts\n  const p = progress(frame, 0, 20, Easings.easeOutExpo);\n  return <Text x={120 + 40 * (1 - p)} y={y} opacity={p}>{label}</Text>;\n}\n\n<Stagger each={4}>\n  {items.map((item, i) => <Row key={item} label={item} y={200 + i * 64} />)}\n</Stagger>"} />
      <h3>{text('docs.chapters.motion-toolkit.on-the-beat')}</h3><p>{text('docs.chapters.motion-toolkit.returns-the-current-the-through-the-beat', [<code>useBeat({'{ bpm }'})</code>, <code>beat</code>, <code>bar</code>, <code>beatInBar</code>, <code>progress</code>, <code>pulse</code>, <code>offset</code>])}</p>
      <DocCode label={t('docs.chapters.motion-toolkit.a-dot-that-pulses-with-the-music')} language="tsx" code={"const { pulse } = useBeat({ bpm: 120 });\nreturn <Rect width={40} height={40} cornerRadius={20} scale={1 + 0.3 * pulse} fill=\"#7cf29c\" />;"} />
      <h3>{text('docs.chapters.motion-toolkit.cues')}</h3><p>{text('docs.chapters.motion-toolkit.takes-a-list-of-objects-sorted-by', [<code>useCue(cues)</code>, <code>{'{ at, …data }'}</code>, <code>at</code>, <code>index</code>, <code>frame</code>, <code>previous</code>, <code>next</code>, <code>cueAt(cues, frame)</code>])}</p>
      <DocCode label={t('docs.chapters.motion-toolkit.captions-that-swap-and-fade-in')} language="tsx" code={"const active = useCue([\n  { at: 0, text: 'Write it.' },\n  { at: 45, text: 'Preview it.' },\n  { at: 90, text: 'Ship it.' },\n]);\nif (!active) return null;\nreturn <Text opacity={progress(active.frame, 0, 10)}>{active.cue.text}</Text>;"} />
      <h3>{text('docs.chapters.motion-toolkit.timecodes')}</h3><p>{text('docs.chapters.motion-toolkit.from-formats-for-an-on-screen-clock', [<code>frameToTimecode(frame, fps)</code>, <code>@celesta/react</code>, <code>HH:MM:SS:FF</code>])}</p>
    </>,
  'text-camera-lines': <>
      <p>{text('docs.chapters.text-camera-lines.titles-that-reveal-themselves-a-camera-that')}</p>
      <h3>{text('docs.chapters.text-camera-lines.text-effects')}</h3>
      <Api caption={t('docs.chapters.text-camera-lines.text-helpers')} rows={[
        [<code>TextReveal</code>, <>{text('docs.chapters.text-camera-lines.api.lines-of-a-string-slide-up-from', [<code>lineHeight</code>, <code>from</code>, <code>stagger</code>, <code>durationInFrames</code>, <code>align</code>])}</>],
        [<code>useTypewriter(text, options)</code>, <>{text('docs.chapters.text-camera-lines.api.the-typed-so-far-its-and-steady', [<code>text</code>, <code>length</code>, <code>done</code>, <code>caretVisible</code>])}</>],
        [<code>useCountUp(to, options)</code>, <>{text('docs.chapters.text-camera-lines.api.a-number-counting-to-with-and', [<code>to</code>, <code>from</code>, <code>delay</code>, <code>durationInFrames</code>, <code>easing</code>, <code>decimals</code>])}</>],
      ]} />
      <DocCode label={t('docs.chapters.text-camera-lines.an-editorial-title-reveal')} language="tsx" code={"<TextReveal x={160} y={300} lineHeight={200} stagger={6}\n  style={{ fontFamily: 'Archivo Black', fontSize: 210,\n    fill: { type: 'solid', color: '#ecefe8' } }}>\n  {'36 DAYS\\nOF CELESTA'}\n</TextReveal>"} />
      <p>{text('docs.chapters.text-camera-lines.celesta-does-not-measure-text-in-react', [<code>length × advance</code>])}</p>
      <h3>{text('docs.chapters.text-camera-lines.camera')}</h3><p>{text('docs.chapters.text-camera-lines.looks-at-a-point-of-a-larger', [<code>{'<Camera>'}</code>, <code>x</code>, <code>y</code>, <code>zoom</code>, <code>rotation</code>, <code>shake</code>])}</p>
      <DocCode label={t('docs.chapters.text-camera-lines.travel-along-a-long-timeline')} language="tsx" code={"<Camera x={interpolate(frame, [0, 120], [0, 4000], { extrapolateRight: 'clamp' })}\n  y={540} zoom={1.1} shake={4}>\n  <TimelineWorld />\n</Camera>"} />
      <h3>{text('docs.chapters.text-camera-lines.lines-and-charts')}</h3><p>{text('docs.chapters.text-camera-lines.draws-a-segment-and-connects-several-both', [<code>{'<Line x1 y1 x2 y2>'}</code>, <code>{'<Polyline points>'}</code>, <code>stroke</code>, <code>strokeWidth</code>, <code>cap</code>, <code>round</code>, <code>join</code>, <code>progress</code>, <code>pointOnPolyline(points, t)</code>])}</p>
      <p>{text('docs.chapters.text-camera-lines.for-other-shapes-takes-with-or-svg', [<code>{'<Path>'}</code>, <code>points</code>, <code>closed</code>, <code>commands</code>, <code>moveTo</code>, <code>lineTo</code>, <code>quadTo</code>, <code>cubicTo</code>, <code>close</code>, <code>stroke</code>, <code>fill</code>, <code>cap</code>, <code>join</code>, <code>miterLimit</code>])}</p>
      <h3>{text('docs.chapters.text-camera-lines.circles-ellipses-and-arrows')}</h3><p>{text('docs.chapters.text-camera-lines.and-are-placed-like-a-put-their', [<code>{'<Circle radius>'}</code>, <code>{'<Ellipse width height>'}</code>, <code>Rect</code>, <code>x</code>, <code>y</code>, <code>fill</code>, <code>stroke</code>, <code>{'<Arrow x1 y1 x2 y2>'}</code>, <code>x2</code>, <code>y2</code>, <code>strokeWidth</code>, <code>headLength</code>, <code>headWidth</code>, <code>heads</code>, <code>end</code>, <code>start</code>, <code>both</code>, <code>Rect</code>])}</p>
      <DocCode label={t('docs.chapters.text-camera-lines.a-labelled-callout')} language="tsx" code={"<Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={80}\n  stroke=\"#FFD84D\" strokeWidth={8} />\n<Arrow x1={600} y1={300} x2={890} y2={480} strokeWidth={6} stroke=\"#FFD84D\" />"} />
    </>,
  'layout': <>
      <h3>{text('docs.chapters.layout.coordinates-and-anchors')}</h3><p>{text('docs.chapters.layout.the-canvas-origin-is-the-top-left', [<code>x</code>, <code>y</code>, <strong />, <code>anchorX</code>, <code>anchorY</code>, <code>0.5</code>, <code>1</code>])}</p>
      <DocCode label={t('docs.chapters.layout.centered-rotated-and-scaled')} language="tsx" code={'<Rect x={960} y={540} anchorX={0.5} anchorY={0.5}\n  width={400} height={400} cornerRadius={48}\n  rotation={12} scale={0.8} opacity={0.9} fill="#a68bbf" />'} />
      <p>{text('docs.chapters.layout.and-override-on-one-axis-multiplies-down', [<code>scaleX</code>, <code>scaleY</code>, <code>scale</code>, <code>opacity</code>])}</p>
      <h3>{text('docs.chapters.layout.shapes')}</h3><p>{text('docs.chapters.layout.draws-a-rectangle-with-an-optional-and', [<code>Rect</code>, <code>cornerRadius</code>, <code>fill</code>, <code>stroke</code>, <code>strokeWidth</code>, <code>#RRGGBB</code>, <code>#RRGGBBAA</code>])}</p>
      <h3>{text('docs.chapters.layout.effects')}</h3><p>{text('docs.chapters.layout.effects-body')}</p>
      <DocCode label={t('docs.chapters.layout.effects-example')} language="tsx" code={'<Group glow={{ color: "#FFCC6680", blur: 12 }}>\n  <Text style={{ fontSize: 96 }}\n    shadow={{ color: "#000000B0", blur: 6, offsetX: 4, offsetY: 6 }}>\n    Celesta\n  </Text>\n</Group>'} />
      <h3>{text('docs.chapters.layout.blend-modes')}</h3><p>{text('docs.chapters.layout.set-on-any-layer-to-change-how', [<code>blendMode</code>, <code>mix-blend-mode</code>, <code>normal</code>, <code>multiply</code>, <code>screen</code>, <code>overlay</code>, <code>add</code>, <code>difference</code>, <code>Group</code>, <code>normal</code>])}</p>
      <DocCode label={t('docs.chapters.layout.a-hud-that-stays-readable-on-light')} language="tsx" code={'<Group blendMode="difference" opacity={0.8}>\n  <Text x={48} y={48} style={{ fill: { type: \'solid\', color: \'#ffffff\' } }}>\n    REC 00:00:12\n  </Text>\n</Group>'} />
      <h3>{text('docs.chapters.layout.group-and-arrange')}</h3><p>{text('docs.chapters.layout.has-no-size-of-its-own-its', [<code>Group</code>, <code>x</code>, <code>y</code>])}</p>
      <Api caption={t('docs.chapters.layout.layout-helpers')} rows={[
        [<code>Center</code>, <>{text('docs.chapters.layout.api.moves-the-origin-to-the-center-of', [<code>SafeArea</code>])}</>],
        [<code>SafeArea</code>, <>{text('docs.chapters.layout.api.insets-its-children-by-a-number-or', [<code>padding</code>, <code>{'{ top, right, bottom, left }'}</code>])}</>],
        [<code>Stack</code>, <>{text('docs.chapters.layout.api.places-each-child-pixels-apart-by-default', [<code>spacing</code>, <code>vertical</code>, <code>horizontal</code>])}</>],
        [<code>Grid</code>, <>{text('docs.chapters.layout.api.places-children-into-of-cells-with-optional', [<code>columns</code>, <code>columnWidth</code>, <code>rowHeight</code>])}</>],
        [<code>Fit</code>, <>{text('docs.chapters.layout.api.scales-content-designed-at-to-or-the', [<code>sourceWidth</code>, <code>sourceHeight</code>, <code>contain</code>, <code>cover</code>])}</>],
      ]} />
      <DocCode label={t('docs.chapters.layout.a-centered-list-inside-a-safe-margin')} language="tsx" code={"<SafeArea padding={96}>\n  <Center>\n    <Stack spacing={96} y={-96}>\n      {['Write', 'Preview', 'Export'].map((step) => (\n        <Text key={step} anchorX={0.5} anchorY={0.5}\n          style={{ fontSize: 64, fill: { type: 'solid', color: '#332f3b' } }}>\n          {step}\n        </Text>\n      ))}\n    </Stack>\n  </Center>\n</SafeArea>"} />
      <p>{text('docs.chapters.layout.helpers-position-child-origins-they-do-not', [<code>0.5</code>])}</p>
      <h3>{text('docs.chapters.layout.clip-a-group')}</h3><p>{text('docs.chapters.layout.give-a-a-to-draw-its-children', [<code>Group</code>, <code>clip</code>, <code>{'{ x, y, width, height, cornerRadius }'}</code>])}</p>
      <DocCode label={t('docs.chapters.layout.text-sliding-up-from-behind-an-edge')} language="tsx" code={"<Group x={120} y={200} clip={{ width: 720, height: 96 }}>\n  <Text y={96 * (1 - reveal)}\n    style={{ fontSize: 88, fill: { type: 'solid', color: '#332f3b' } }}>\n    Layer it.\n  </Text>\n</Group>"} />
    </>,
  'text-fonts': <>
      <h3>{text('docs.chapters.text-fonts.text')}</h3><p>{text('docs.chapters.text-fonts.renders-its-children-which-must-be-strings', [<code>Text</code>, <code>style</code>])}</p>
      <DocCode label={t('docs.chapters.text-fonts.an-outlined-wrapping-caption')} language="tsx" code={"<Text x={960} y={900} anchorX={0.5} anchorY={0.5} maxWidth={1400}\n  style={{\n    fontFamily: 'Hiragino Sans',\n    fontSize: 64,\n    fontWeight: 700,\n    lineHeight: 80,\n    align: 'center',\n    fill: { type: 'solid', color: '#ffffff' },\n    stroke: { paint: { type: 'solid', color: '#57456c' }, width: 8 },\n  }}>\n  Words that wrap onto a second line when they reach maxWidth.\n</Text>"} />
      <ul>
        <li>{text('docs.chapters.text-fonts.names-a-font-installed-on-the-computer', [<code>fontFamily</code>, <code>{'<Font>'}</code>, <code>fontWeight</code>, <code>font</code>])}</li>
        <li>{text('docs.chapters.text-fonts.selects-the-language-used-for-fallback-fonts', [<code>lang</code>, <code>fontFamily</code>, <code>{'<Composition lang="ja-JP">'}</code>, <code>Composition</code>, <code>Group</code>, <code>Sequence</code>, <code>lang</code>, <code>useTextMetrics()</code>, <code>useFitText()</code>, <code>{'<Text lang="zh-Hant">'}</code>, <code>style.lang</code>, <code>lang</code>, <code>ja-JP</code>, <code>ko-KR</code>, <code>zh-Hans</code>, <code>zh-Hant</code>, <code>zh-Hant-HK</code>, <code>lang</code>])}</li>
      </ul>
      <DocCode label={t('docs.chapters.text-fonts.a-japanese-default-with-a-chinese-group')} language="tsx" code={"<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang=\"ja-JP\">\n  <Text>漢字とかな</Text>\n  <Group lang=\"zh-Hant\">\n    <Text y={80}>繁體中文</Text>\n    <Text y={160} lang=\"ko-KR\">한국어</Text>\n  </Group>\n</Composition>"} />
      <p>{text('docs.chapters.text-fonts.runs-before-is-mounted-pass-explicitly-to', [<code>prepare()</code>, <code>{'<Composition>'}</code>, <code>lang</code>, <code>measureText()</code>, <code>{"measureText('漢字とかな', { lang: 'ja-JP' })"}</code>])}</p>
      <p>{text('docs.chapters.text-fonts.to-use-a-font-file-that-is', [<code>{'<Font>'}</code>, <code>src</code>, <code>fontFamily</code>, <code>src</code>, <code>http://</code>, <code>https://</code>])}</p>
      <DocCode label={t('docs.chapters.text-fonts.load-a-font-file-next-to-the')} language="tsx" code={"<Composition width={1920} height={1080} fps={30} durationInFrames={90}>\n  <Assets>\n    <Font src=\"./fonts/MPLUSRounded1c-Bold.ttf\" />\n  </Assets>\n  <Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n    Hello\n  </Text>\n</Composition>"} />
      <p>{text('docs.chapters.text-fonts.can-also-be-a-web-font-stylesheet', [<code>src</code>, <code>@font-face</code>, <code>fontFamily</code>])}</p>
      <DocCode label={t('docs.chapters.text-fonts.load-a-google-fonts-family')} language="tsx" code={"<Assets>\n  <Font src=\"https://fonts.googleapis.com/css2?family=M+PLUS+Rounded+1c:wght@400;700\" />\n</Assets>\n<Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>\n  こんにちは\n</Text>"} />
      <ul>
        <li>{text('docs.chapters.text-fonts.wraps-text-at-word-boundaries-and-positions', [<code>maxWidth</code>, <code>align</code>, <code>\n</code>])}</li>
        <li>{text('docs.chapters.text-fonts.japanese-normally-wraps-between-most-characters-splitting', [<code>lineBreak: 'phrase'</code>, <a href="https://github.com/google/budoux" />, <code>lang</code>, <code>maxWidth</code>, <code>lineBreak</code>])}</li>
        <li>{text('docs.chapters.text-fonts.single-line-text-is-anchored-vertically-by', [<code>anchorY={'{0.5}'}</code>])}</li>
        <li>{text('docs.chapters.text-fonts.anchors-text-on-its-first-line-s', [<code>anchorY="baseline"</code>, <code>y</code>])}</li>
      </ul>
      <h3>{text('docs.chapters.text-fonts.text-measurement')}</h3>
      <p>{text('docs.chapters.text-fonts.text-measurement-body')}</p>
      <p>{text('docs.chapters.text-fonts.text-fitting-body')}</p>
      <DocCode label="fitted-text.tsx" language="tsx" code={fittedTextScene.trim()} />
    </>,
  'media': <>
      <p>{text('docs.chapters.media.and-read-files-through-relative-paths-start', [<code>Image</code>, <code>Video</code>, <code>Audio</code>, <code>src</code>])}</p>
      <p>{text('docs.chapters.media.can-also-be-an-or-url-celesta', [<code>src</code>, <code>http://</code>, <code>https://</code>, <code>preloadMedia()</code>])}</p>
      <DocCode label="media.tsx" language="tsx" code={mediaScene.trim()} />
      <ul>
        <li>{text('docs.chapters.media.an-or-is-drawn-at-its-own', [<code>Image</code>, <code>Video</code>, <code>x</code>, <code>y</code>, <code>scale</code>, <code>Fit</code>])}</li>
        <li>{text('docs.chapters.media.and-accept-seconds-into-the-file-and', [<code>Video</code>, <code>Audio</code>, <code>startFrom</code>, <code>playbackRate</code>, <code>Audio</code>, <code>volume</code>, <code>muted</code>])}</li>
        <li>{text('docs.chapters.media.media-placed-inside-a-starts-when-the', [<code>Sequence</code>])}</li>
        <li>{text('docs.chapters.media.and-an-audio-can-be-a-number', [<code>volume</code>, <code>playbackRate</code>, <code>{'{ value, timescale }'}</code>])}</li>
      </ul>
      <p>{text('docs.shared.audio-frame-keyframes')}</p>
      <h3>{text('docs.chapters.media.size-a-scene-to-its-media')}</h3><p>{text('docs.chapters.media.the-optional-export-runs-once-before-rendering', [<code>prepare()</code>, <code>async</code>, <code>preloadMedia()</code>, <code>mediaDurationInFrames()</code>])}</p>
    </>,
  'timelines': <>
      <Note title={t('docs.chapters.timelines.deprecated')}>{text('docs.chapters.timelines.json-projects-are-deprecated-use-react-compositions', [<code>.celesta.json</code>])}</Note>
      <p>{text('docs.chapters.timelines.a-file-describes-project-settings-assets-characters', [<code>.celesta.json</code>])}</p>
      <ul><li>{text('docs.chapters.timelines.defines-dimensions-frame-rate-audio-sample-rate', [<code>settings</code>])}</li><li>{text('docs.chapters.timelines.registers-source-media-by-id-or-media', [<code>assets</code>, <code>video</code>, <code>audio</code>, <code>image</code>, <code>font</code>])}</li><li>{text('docs.chapters.timelines.contains-video-audio-overlay-or-dialogue-tracks', [<code>tracks</code>])}</li><li>{text('docs.chapters.timelines.each-item-has-a-with-a-start', [<code>range</code>, <code>transform</code>, <code>opacity</code>, <code>blendMode</code>])}</li></ul>
      <p>{text('docs.chapters.timelines.time-is-represented-as-divide-the-value', [<code>{'{ value, timescale }'}</code>, <code>{'{ "value": 10, "timescale": 1 }'}</code>])}</p>
      <p>{text('docs.chapters.timelines.copy-the-project-below-into-in-your', [<code>project.celesta.json</code>, <strong />])}</p><details className="doc-details">{text('docs.chapters.timelines.view-the-complete-built-in-title-project', [<summary />, <DocCode label="examples/editor-demo.celesta.json" language="json" code={timelineExample.trim()} />])}</details>
      <p>{text('docs.chapters.timelines.is-an-empty-starting-point-for-a', [<code>examples/minimal.celesta.json</code>, <strong />])}</p>
      <DocCode label={t('docs.chapters.timelines.export-json-project')} code="celesta-exporter project.celesta.json output.mp4" />
      <h3>{text('docs.chapters.timelines.animate-with-keyframes')}</h3><p>{text('docs.chapters.timelines.in-a-json-transform-places-the-item', [<code>position</code>, <code>opacity</code>, <code>volume</code>, <code>easing</code>])}</p>
      <DocCode label={t('docs.chapters.timelines.fade-an-item-in-over-half-a')} language="json" code={'"opacity": {\n  "type": "keyframes",\n  "keyframes": [\n    { "time": { "value": 0, "timescale": 30 }, "value": 0 },\n    { "time": { "value": 15, "timescale": 30 }, "value": 1, "easing": "ease-out" }\n  ]\n}'} />
      <p>{text('docs.chapters.timelines.easing-names-match-the-react-in-kebab', [<code>Easings</code>, <code>ease-in-out-cubic</code>, <code>ease-out-back</code>])}</p>
      <h3>{text('docs.chapters.timelines.dialogue-in-a-json-project')}</h3><p>{text('docs.chapters.timelines.a-json-project-declares-characters-in-and', [<code>characters</code>, <code>dialogue</code>])}</p>
      <details className="doc-details">{text('docs.chapters.timelines.view-the-complete-json-dialogue-project', [<summary />, <DocCode label="dialogue.celesta.json" language="json" code={dialogueProject.trim()} />])}</details>
      <p>{text('docs.chapters.timelines.json-lines-can-lip-sync-as-well', [<code>lipSync</code>, <code>{'"lipSync": [{ "time": { "value": 0, "timescale": 30 }, "shape": "a" }]'}</code>, <code>loadLipSync()</code>])}</p>
      <h3>{text('docs.chapters.timelines.combine-a-project-with-react')}</h3><p>{text('docs.chapters.timelines.a-react-composition-can-include-pass-its', [<code>{'<ProjectTimeline />'}</code>])}</p>
      <DocCode label={t('docs.chapters.timelines.export-with-companion-project')} code="celesta-exporter --react with-project.tsx --project project.celesta.json output.mp4" />
      <details className="doc-details">{text('docs.chapters.timelines.view-the-companion-react-composition', [<summary />, <DocCode label="with-project.tsx" language="tsx" code={withProject.trim()} />])}</details><p>{text('docs.chapters.timelines.save-this-as-beside-your-json-file', [<code>with-project.tsx</code>, <a href={docPath('export')} />, <code>{'<ProjectTrack id="…" />'}</code>])}</p>
    </>,
  'dialogue': <>
      <p>{text('docs.chapters.dialogue.dialogue-scenes-are-built-from-three-pieces')}</p>
      <ul>
        <li>{text('docs.chapters.dialogue.a-character-defines-a-portrait-one-image', [<strong />])}</li>
        <li>{text('docs.chapters.dialogue.a-character-view-places-the-portrait-on', [<strong />])}</li>
        <li>{text('docs.chapters.dialogue.a-dialogue-line-shows-a-subtitle-plays', [<strong />])}</li>
      </ul>
      <h3>{text('docs.chapters.dialogue.your-first-conversation-in-react')}</h3><p>{text('docs.chapters.dialogue.save-the-file-below-with-two-portrait')}</p>
      <DocCode label="dialogue.tsx" language="tsx" code={dialogueScene.trim()} />
      <ol>
        <li>{text('docs.chapters.dialogue.declare-the-character-inside-the-ref-lets', [<strong />, <code>{'<Assets>'}</code>, <code>subtitle</code>, <code>Text</code>])}</li>
        <li>{text('docs.chapters.dialogue.place-a-view-with-it-shows-the', [<strong />, <code>{'<CharacterView>'}</code>, <code>defaultExpression</code>, <code>x</code>, <code>y</code>, <code>scale</code>])}</li>
        <li>{text('docs.chapters.dialogue.time-each-line-by-wrapping-in-a', [<strong />, <code>{'<Dialogue>'}</code>, <code>Sequence</code>, <code>expression</code>])}</li>
      </ol>
      <Note title={t('docs.chapters.dialogue.one-view-many-lines')}>{text('docs.chapters.dialogue.a-points-to-a-view-not-to', [<code>Dialogue</code>, <code>Character</code>])}</Note>
      <p>{text('docs.chapters.dialogue.adjust-a-line-with-or-just-as', [<code>volume</code>, <code>startFrom</code>, <code>playbackRate</code>, <code>muted</code>, <code>Audio</code>, <code>x</code>, <code>y</code>, <code>opacity</code>, <code>Dialogue</code>, <code>Transition</code>])}</p>
      <h3>{text('docs.chapters.dialogue.automatic-lip-sync')}</h3><p>{text('docs.chapters.dialogue.listens-to-a-voice-recording-and-spreads', [<code>loadLipSync()</code>, <code>a</code>, <code>i</code>, <code>u</code>, <code>e</code>, <code>o</code>, <code>closed</code>, <code>prepare()</code>, <code>Dialogue</code>, <code>CharacterView</code>, <code>lipSync</code>])}</p>
      <DocCode label="lip-sync.tsx" language="tsx" code={lipSyncScene.trim()} />
      <ul>
        <li>{text('docs.chapters.dialogue.voices-must-be-uncompressed-wav-files-write', [<strong />])}</li>
        <li>{text('docs.chapters.dialogue.psd-portraits-name-their-mouth-layers-by', [<strong />, <code>face/mouth/a</code>, <code>layers</code>, <code>.pfv</code>, <code>loadPsdPreset()</code>])}</li>
        <li>{text('docs.chapters.dialogue.image-portraits-can-lip-sync-too-give', [<strong />, <code>portrait.lipSync</code>])}</li>
        <li>{text('docs.chapters.dialogue.to-drive-a-mouth-yourself-pass-or', [<code>mouth="a"</code>, <code>CharacterView</code>, <code>Dialogue</code>, <code>useLipSync(track)</code>])}</li>
      </ul>
      <h3>{text('docs.chapters.dialogue.dialogue-series')}</h3>
      <p>{text('docs.chapters.dialogue.dialogue-series-body')}</p>
      <DocCode label="dialogue-series.tsx" language="tsx" code={dialogueSeriesScene.trim()} />
      <p>{text('docs.chapters.dialogue.dialogue-subtitles-body')}</p>
      <p>{text('docs.chapters.dialogue.voicevox-link', [<a href={docPath('voicevox')} />])}</p>
      <h3>{text('docs.chapters.dialogue.see-it-with-real-artwork')}</h3><p>{text('docs.chapters.dialogue.the-repository-s-is-a-complete-json', [<code>packages/react/examples/with-dialogue-series.tsx</code>, <code>packages/react/examples/with-lip-sync.tsx</code>, <a href={repository} />, <strong />, <strong />])}</p>
    </>,
  'data': <>
      <h3>{text('docs.chapters.data.load-data-before-rendering')}</h3><p>{text('docs.chapters.data.each-frame-is-rendered-synchronously-so-a', [<code>async prepare()</code>])}</p>
      <DocCode label={t('docs.chapters.data.fetch-once-use-on-every-frame')} language="tsx" code={"let headlines: string[] = ['Hello from Celesta'];\n\nexport async function prepare() {\n  try {\n    const response = await fetch('https://example.com/headlines.json');\n    headlines = (await response.json()) as string[];\n  } catch (error) {\n    console.error('Using the fallback headline', error);\n  }\n}"} />
      <p>{text('docs.chapters.data.keep-a-fallback-value-so-the-scene', [<code>loadLipSync()</code>, <code>loadPsdPreset()</code>, <code>preloadMedia()</code>, <code>prepare()</code>])}</p>
      <h3>{text('docs.chapters.data.reusable-components')}</h3>
      <p>{text('docs.chapters.data.reusable-components-body')}</p>
      <DocCode label="data.tsx" language="tsx" code={dataScene.trim()} />
      <Note title={t('docs.chapters.data.legacy-project-data')}>{text('docs.chapters.data.legacy-project-data-body', [<a href={docPath('timelines')} />])}</Note>
    </>,
  'export': <>
      <h3>{text('docs.chapters.export.in-your-browser')}</h3><p>{text('docs.chapters.export.open-the-web-editor-change-or-open', [<a href={`${homePath}#playground`} />, <strong />, <strong />, <code>prepare()</code>])}</p>
      <h3>{text('docs.chapters.export.from-the-app')}</h3><p>{text('docs.chapters.export.choose-export-and-select-an-mp4-destination', [<strong />, <kbd>I</kbd>, <kbd>O</kbd>, <strong />])}</p>
      <h3>{text('docs.chapters.export.from-the-command-line')}</h3><ExportCommands />
      <p>{text('docs.chapters.export.time-values-accept-seconds-or-the-selected', [<code>MM:SS.mmm</code>, <code>HH:MM:SS.mmm</code>])}</p>
      <h3>{text('docs.chapters.export.on-linux-without-a-gpu')}</h3><p>{text('docs.chapters.export.on-linux-the-exporter-renders-through-vulkan', [<a href={docPath('build-from-source')} />])}</p>
      <DocCode label={t('docs.chapters.export.install-software-vulkan')} code="sudo apt-get install -y mesa-vulkan-drivers" />
      <p>{text('docs.chapters.export.software-rendering-is-much-slower-than-a')}</p>
      <DocCode label={t('docs.chapters.export.export-one-frame')} code="cargo run -p celesta-exporter --release -- --react --driver vulkan packages/react/examples/title.tsx --frame 0 frame.png" />
      <p>{text('docs.chapters.export.minimal-containers-often-have-no-fonts-installed', [<code>fonts-dejavu-core</code>, <code>fonts-noto-cjk</code>, <code>{'<Font>'}</code>])}</p>
      <p>{text('docs.chapters.export.on-a-software-renderer-leaves-the-rgb', [<code>--color-conversion auto</code>, <code>--render-quality draft</code>, <a href={`${repository}/tree/main/packaging/linux`} />, <code>/work</code>, <code>--user</code>])}</p>
      <DocCode label={t('docs.chapters.export.build-the-container-image')} code="docker build -f packaging/linux/Dockerfile -t celesta-exporter ." />
      <DocCode label={t('docs.chapters.export.export-in-a-container')} code={'docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react first-scene.tsx output.mp4'} />
      <Note title={t('docs.chapters.export.before-you-render')}>{text('docs.chapters.export.mp4-output-uses-h-264-video-and')}</Note>
    </>,
  'reference': <>
      <p>{text('docs.chapters.reference.import-these-apis-from-random-numbers-noise', [<code>@celesta/react</code>, <code>@celesta/math</code>, <a href={docPath('math')} />, <code>@celesta/code</code>, <a href={docPath('code')} />])}</p>
      <Api caption={t('docs.chapters.reference.layers')} rows={[
        [<code>Composition</code>, <>{text('docs.chapters.reference.api.set-and-optional-supplies-the-default-text', [<code>width</code>, <code>height</code>, <code>fps</code>, <code>durationInFrames</code>, <code>lang</code>])}</>],
        [<code>Rect</code>, <>{text('docs.chapters.reference.api.a-rectangle-with-and-optional-and', [<code>width</code>, <code>height</code>, <code>fill</code>, <code>stroke</code>, <code>strokeWidth</code>, <code>cornerRadius</code>])}</>],
        [<code>Text</code>, <>{text('docs.chapters.reference.api.text-from-string-or-number-children-styled', [<code>style</code>, <code>maxWidth</code>, <code>lang</code>, <code>style.lang</code>, <a href={docPath('text-fonts')} />, text('docs.chapters.reference.text-fonts')])}</>],
        [<code>Group</code>, <>{text('docs.chapters.reference.api.applies-a-shared-position-scale-rotation-and', [<code>lang</code>])}</>],
        [<><code>Image</code> / <code>Video</code> / <code>Audio</code></>, <>{text('docs.chapters.reference.api.local-media-through-see', [<code>src</code>, <a href={docPath('media')} />, text('docs.chapters.reference.images-video-sound')])}</>],
        [<code>Font</code>, <>{text('docs.chapters.reference.api.loads-a-local-font-file-through-so', [<code>src</code>, <code>Text</code>, <a href={docPath('text-fonts')} />, text('docs.chapters.reference.text-fonts-label')])}</>],
      ]} />
      <Api caption={t('docs.chapters.reference.time-and-motion')} rows={[
        [<code>Sequence</code>, <>{text('docs.chapters.reference.api.places-children-at-a-frame-offset-with', [<code>from</code>, <code>durationInFrames</code>, <code>lang</code>])}</>],
        [<code>FreezeFrame</code>, <>{text('docs.chapters.reference.api.draws-its-children-as-the-composition-looked', [<code>frame</code>])}</>],
        [<code>Transition</code>, <>{text('docs.chapters.reference.api.a-fade-slide-or-scale-or-several')}</>],
        [<><code>Series</code> / <code>computeSeries</code></>, <>{text('docs.chapters.reference.api.scenes-back-to-back-by-length-see', [<a href={docPath('motion-toolkit')} />, text('docs.chapters.reference.scenes-cues-beats')])}</>],
        [<><code>TransitionSeries</code> / <code>computeTransitionSeries</code></>, <>{text('docs.chapters.reference.api.transition-series', [<a href={docPath('motion-toolkit')} />, text('docs.chapters.reference.scenes-cues-beats')])}</>],
        [<code>Stagger</code>, <>{text('docs.chapters.reference.api.starts-each-child-a-fixed-number-of')}</>],
        [<code>progress</code>, <>{text('docs.chapters.reference.api.a-clamped-eased-0-1-value-for')}</>],
        [<><code>useBeat()</code> / <code>beatAt</code></>, <>{text('docs.chapters.reference.api.beats-bars-and-a-pulse-for-a')}</>],
        [<><code>useCue()</code> / <code>cueAt</code></>, <>{text('docs.chapters.reference.api.the-cue-in-effect-from-a-list', [<code>{'{ at, …data }'}</code>])}</>],
        [<><code>TextReveal</code>, <code>useTypewriter()</code>, <code>useCountUp()</code></>, <>{text('docs.chapters.reference.api.masked-line-reveals-typing-and-counting-numbers')}</>],
        [<code>Camera</code>, <>{text('docs.chapters.reference.api.look-at-a-point-of-a-larger')}</>],
        [<><code>Line</code> / <code>Polyline</code> / <code>Path</code> / <code>pointOnPolyline</code></>, <>{text('docs.chapters.reference.api.segments-curves-and-filled-shapes-polylines-can')}</>],
        [<><code>Circle</code> / <code>Ellipse</code> / <code>Arrow</code></>, <>{text('docs.chapters.reference.api.diagram-shapes-a-circle-is-a-rounded', [<code>Rect</code>, <code>Path</code>, <code>Rect</code>])}</>],
        [<><code>interpolate</code> / <code>Easings</code></>, <>{text('docs.chapters.reference.api.map-a-frame-to-a-value-with')}</>],
        [<code>interpolateColor</code>, <>{text('docs.chapters.reference.api.map-a-frame-to-a-color-blending', [<code>#RRGGBBAA</code>])}</>],
        [<code>frameKeyframes</code>, <>{text('docs.shared.audio-frame-keyframes')}</>],
        [<code>spring</code>, <>{text('docs.chapters.reference.api.a-physics-based-value-that-settles-from')}</>],
        [<><code>useCurrentFrame()</code> / <code>useCurrentTime()</code></>, <>{text('docs.chapters.reference.api.the-current-frame-or-the-exact-time')}</>],
        [<code>useVideoConfig()</code>, <>{text('docs.chapters.reference.api.the-composition-s-or-sequence-s-dimensions')}</>],
        [<><code>timecodeToFrame()</code> / <code>frameToTimecode()</code></>, <>{text('docs.chapters.reference.api.convert-a-style-timecode-to-a-frame', [<code>MM:SS.mmm</code>, <code>HH:MM:SS:FF</code>])}</>],
      ]} />
      <Api caption={t('docs.chapters.reference.characters')} rows={[
        [<><code>Assets</code> / <code>Character</code></>, <>{text('docs.chapters.reference.api.declare-a-character-s-portrait-expressions-lip')}</>],
        [<code>CharacterView</code>, <>{text('docs.chapters.reference.api.places-a-character-s-portrait-accepts-and', [<code>expression</code>, <code>mouth</code>, <code>lipSync</code>])}</>],
        [<><code>DialogueSeries</code> / <code>planDialogue</code></>, <>{text('docs.chapters.reference.api.dialogue-series', [<a href={docPath('dialogue')} />])}</>],
        [<code>Dialogue</code>, <>{text('docs.chapters.reference.api.shows-a-line-as-a-subtitle-plays', [<code>audio</code>])}</>],
        [<><code>loadLipSync</code> / <code>useLipSync</code></>, <>{text('docs.chapters.reference.api.generate-mouth-shapes-from-a-wav-recording')}</>],
        [<code>loadPsdPreset</code>, <>{text('docs.chapters.reference.api.load-visible-psd-layers-from-a-psdtool', [<code>.pfv</code>])}</>],
      ]} />
      <Api caption={t('docs.chapters.reference.layout-data-and-tools')} rows={[
        [<><code>TextBox</code> / <code>fitText</code> / <code>useFitText</code> / <code>measureText</code> / <code>useTextMetrics</code></>, <>{text('docs.chapters.reference.api.text-measure-and-fit', [<a href={docPath('text-fonts')} />])}</>],
        [<><code>Center</code>, <code>SafeArea</code>, <code>Stack</code>, <code>Grid</code>, <code>Fit</code></>, <>{text('docs.chapters.reference.api.arrange-child-layers-see', [<a href={docPath('layout')} />, text('docs.chapters.reference.layout-helpers')])}</>],
        [<><code>preloadMedia</code> / <code>mediaDurationInFrames</code></>, <>{text('docs.chapters.reference.api.read-a-media-file-s-length-and', [<code>prepare()</code>])}</>],
        [<><code>DebugOverlay</code> / <code>DebugBounds</code> / <code>useIsPreview()</code></>, <>{text('docs.chapters.reference.api.preview-only-guides-that-never-appear-in')}</>],
      ]} />
      <h3>{text('docs.chapters.reference.legacy-api')}</h3><p>{text('docs.chapters.reference.legacy-api-body')}</p>
      <Api caption={t('docs.chapters.reference.legacy-api')} rows={[
        [<><code>loadProject</code>, <code>ProjectProvider</code>, <code>useProjectProperty</code></>, <>{text('docs.chapters.reference.api.read-values-from-a-json-project')}</>],
        [<><code>ProjectTimeline</code> / <code>ProjectTrack</code></>, <>{text('docs.chapters.reference.api.draw-a-companion-json-project-s-timeline')}</>],
        [<><code>registerComponent</code> / <code>defineProjectProperties</code></>, <>{text('docs.chapters.reference.api.make-components-and-properties-available-to-json')}</>],
      ]} />
    </>,
  'examples': <>
      <p><a className="button button-secondary" href={showcasePath('', currentLocale)}>{t('showcase.explore')} →</a></p>
      <div className="doc-example-grid">
        <a href={docPath('react-compositions')}><span>{text('docs.chapters.examples.01-react')}</span><strong>{text('docs.chapters.examples.a-title-in-motion')}</strong><p>{text('docs.chapters.examples.follow-the-complete-fading-title-example-in')}</p></a>
        <a href={docPath('media')}><span>{text('docs.chapters.examples.02-media')}</span><strong>{text('docs.chapters.examples.footage-and-sound')}</strong><p>{text('docs.chapters.examples.combine-image-video-and-audio-layers-in')}</p></a>
        <a href={docPath('dialogue')}><span>{text('docs.chapters.examples.03-dialogue')}</span><strong>{text('docs.chapters.examples.give-it-a-voice')}</strong><p>{text('docs.chapters.examples.pair-a-portrait-subtitles-and-a-voice')}</p></a>
      </div>
      <h3>{text('docs.chapters.examples.included-with-the-app')}</h3><p>{text('docs.chapters.examples.copy-an-example-into-your-own-project')}</p><ul><li>{text('docs.chapters.examples.macos-in-finder-right-click-celesta-in', [<strong />, <strong />, <code>Contents/Resources/examples</code>])}</li><li>{text('docs.chapters.examples.windows-open-the-folder-beside-in-the', [<strong />, <code>examples</code>, <code>Celesta.exe</code>])}</li></ul>
      <p>{text('docs.chapters.examples.the-repository-s-directory-contains-more-scenes', [<code>packages/react/examples</code>, <strong />])}</p>
      <a className="doc-text-link" href={`${repository}/tree/main/packages/react/examples`}>{text('docs.chapters.examples.browse-all-source-examples-on-github')}</a>
      <p>{text('docs.chapters.examples.you-can-also-open-the-web-editor', [<a href={`${homePath}#playground`} />, <code>.tsx</code>])}</p>
    </>,
  'build-from-source': <SourceBuild />,
  'troubleshooting': <>
      <details className="doc-details" open><summary>{text('docs.chapters.troubleshooting.the-app-cannot-find-its-bundled-runtime')}</summary><p>{text('docs.chapters.troubleshooting.on-macos-copy-the-complete-app-into', [<code>runtime</code>, <code>Celesta.exe</code>])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.a-source-build-cannot-find-ffmpeg')}</summary><p>{text('docs.chapters.troubleshooting.confirm-that-the-8-1-x-development', [<code>PKG_CONFIG_PATH</code>, <code>VCPKG_ROOT</code>, <code>PKG_CONFIG_PATH</code>, <code>/opt/ffmpeg8/lib/pkgconfig</code>, <a href={docPath('build-from-source')} />])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.my-media-is-missing')}</summary><p>{text('docs.chapters.troubleshooting.celesta-currently-supports-local-media-files-not')}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.the-preview-did-not-change-after-saving')}</summary><p>{text('docs.chapters.troubleshooting.react-compositions-reload-automatically-for-json-projects', [<strong />])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.my-text-uses-the-wrong-font')}</summary><p>{text('docs.chapters.troubleshooting.must-match-the-family-name-of-a', [<code>fontFamily</code>, <code>{'<Font>'}</code>, <code>celesta-exporter</code>, <code>{'<Font>'}</code>])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.the-mouth-does-not-move')}</summary><p>{text('docs.chapters.troubleshooting.lip-sync-needs-an-uncompressed-wav-voice', [<code>loadLipSync()</code>, <code>prepare()</code>, <code>lipSync</code>, <code>layers</code>])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.my-editor-cannot-resolve-celesta-react')}</summary><p>{text('docs.chapters.troubleshooting.open-the-composition-in-celesta-and-choose', [<strong />, <code>tsconfig.json</code>, <code>./.celesta/tsconfig.json</code>])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.mp4-export-will-not-start')}</summary><p>{text('docs.chapters.troubleshooting.use-non-zero-even-numbered-dimensions-and', [<code>--overwrite</code>])}</p></details>
      <details className="doc-details"><summary>{text('docs.chapters.troubleshooting.graphics-error')}</summary><p>{text('docs.chapters.troubleshooting.graphics-error-body')}</p></details>
      <h3>{text('docs.chapters.troubleshooting.still-stuck')}</h3><p>{text('docs.chapters.troubleshooting.open-an-issue-with-your-operating-system')}</p>
      <a className="doc-text-link" href={`${repository}/issues`}>{text('docs.chapters.troubleshooting.report-a-problem-on-github')}</a>
    </>,
};
