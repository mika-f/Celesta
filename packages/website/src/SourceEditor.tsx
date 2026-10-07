import { useEffect, useId } from 'react';
import Editor, { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker';
import JsonWorker from 'monaco-editor/language/json/json.worker.js?worker';
import TypeScriptWorker from 'monaco-editor/language/typescript/ts.worker.js?worker';
import { t } from './i18n';

self.MonacoEnvironment = {
  getWorker(_, label) {
    if (label === 'json') return new JsonWorker();
    if (label === 'typescript' || label === 'javascript') return new TypeScriptWorker();
    return new EditorWorker();
  },
};
loader.config({ monaco });

for (const defaults of [monaco.typescript.typescriptDefaults, monaco.typescript.javascriptDefaults]) {
  defaults.setCompilerOptions({ jsx: monaco.typescript.JsxEmit.ReactJSX, target: monaco.typescript.ScriptTarget.ESNext, allowNonTsExtensions: true });
  // Compilation reports project errors; the editor has no Celesta/React type declarations.
  defaults.setDiagnosticsOptions({ noSemanticValidation: true });
}
monaco.editor.defineTheme('celesta', {
  base: 'vs-dark', inherit: true, rules: [],
  colors: { 'editor.background': '#0a0a0a', 'editor.foreground': '#ededeb', 'editor.selectionBackground': '#b5a2e74d' },
});

export default function SourceEditor({ name, source, readOnly, onChange }: {
  name: string; source: string; readOnly: boolean; onChange: (source: string) => void;
}) {
  const id = useId();
  const root = `file:///celesta/${encodeURIComponent(id)}/`;
  useEffect(() => () => {
    // Monaco retains inactive file models so switching files preserves undo and scroll.
    for (const model of monaco.editor.getModels()) {
      if (model.uri.path.startsWith(monaco.Uri.parse(root).path)) model.dispose();
    }
  }, [root]);

  const language = name.endsWith('.json') ? 'json' : /\.[cm]?jsx?$/.test(name) ? 'javascript' : 'typescript';
  return <Editor path={`${root}${name.split('/').map(encodeURIComponent).join('/')}`} language={language} value={source} theme="celesta"
    loading={t('playground.editor-loading')}
    onChange={value => { if (value !== undefined) onChange(value); }}
    options={{
      readOnly, ariaLabel: t('playground.composition-source-code'), automaticLayout: true, editContext: false,
      fontFamily: "'SF Mono', Menlo, Consolas, monospace", fontSize: 12.5, lineHeight: 21,
      minimap: { enabled: false }, scrollBeyondLastLine: false, tabSize: 2,
      padding: { top: 16, bottom: 16 }, fixedOverflowWidgets: true,
    }} />;
}
