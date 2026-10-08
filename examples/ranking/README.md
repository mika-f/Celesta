# Ranking card template

One composition, several videos. `film.tsx` declares its inputs with
`defineProjectProperties()` — title, subtitle, accent color, light or dark
theme, and the data file — and each file in `variants/` picks values for
them. `prepare()` reads the data file named by the `data` property, and the
composition's length follows the number of rows. 1280 × 720 / 30 fps.

From the repository root:

```sh
cargo run -p celesta-exporter --release -- --react examples/ranking/film.tsx spring.mp4 \
  --props-file examples/ranking/variants/spring.json
cargo run -p celesta-exporter --release -- --react examples/ranking/film.tsx autumn.mp4 \
  --props-file examples/ranking/variants/autumn.json
```

`--props` overrides single values without another file:

```sh
cargo run -p celesta-exporter --release -- --react examples/ranking/film.tsx autumn-ja.mp4 \
  --props-file examples/ranking/variants/autumn.json --props '{"title":"秋の人気ドリンク"}'
```

Preview a variant with the same values (edits to the variant file reload the
preview):

```sh
cargo run -p celesta-editor -- examples/ranking/film.tsx --props-file examples/ranking/variants/spring.json
```

Without any values the template renders `data/sample.json` with its declared
defaults. Relative paths in a variant file, such as `"data": "spring-drinks.json"`,
resolve from that file's folder. Noto Sans JP is loaded from Google Fonts
(SIL Open Font License).
