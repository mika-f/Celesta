# Celesta icon atelier

Celesta のアイコンを検討する Vite + React + TypeScript + Tailwind CSS の小さな工房。

```sh
pnpm --dir packages/logos install
pnpm --dir packages/logos dev
```

- **Starlight** — 星と再生ボタン
- **Moon ribbon** — 三日月で描いた C と再生ボタン
- **Little bloom** — 花と再生ボタン

4 色のパレット、ライト／ダーク／透明プレビュー、角丸背景の切り替え、小サイズでの見え方を確認できます。選んだアイコンは SVG または 1024 × 1024 の透過 PNG として保存できます。プレビュー用の背景とワードマークは書き出しに含みません。

アイコンは `src/icons.ts` の SVG で定義しているため、外部画像やフォントへの依存はありません。

```sh
pnpm --dir packages/logos build
pnpm --dir packages/logos preview
```
