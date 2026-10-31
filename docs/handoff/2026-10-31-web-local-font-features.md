# `fontFeatures` on `local()` faces in the browser renderer (2026-10-31)

Closes the first remaining difference in
[`2026-10-31-native-font-face-local.md`](2026-10-31-native-font-face-local.md).

## Problem

Canvas text has no `font-feature-settings`, so `featureFamily` in
`packages/web/src/fonts.ts` draws text with `fontFeatures` in copies of the
family's loaded faces that carry the `featureSettings` descriptor. It made
each copy from the face's file bytes. A face that a font stylesheet draws
with `local()` has no bytes, so the browser preview threw
`fontFeatures cannot use "<family>": its font CSS gives no url() to load it
from.` The native renderer applies the features to the same face.

## Why a `local()` copy works

A `FontFace` takes its descriptors with any source. Chromium merges the
face's `featureSettings` into the font description in
`CSSFontFace::GetFontData` before it tries the sources, so a `local()`
source gets them as a file does. The PR's screenshots check this in the
browser preview: Hoefler Text's `ffi`, `ffl`, and `fj` ligatures are gone
with `liga: false`.

## Implementation

- `faceSources` keeps each loaded face's source, the file's bytes or the
  `local("A"), local("B")` string the face was made from, with its
  descriptors. `withSource` makes a face and records it, for files, font
  stylesheet files, and `local()` names alike.
- `featureFamily` makes each copy from that source with the
  `featureSettings` descriptor. A `FontFace` with a string source stays
  `unloaded` until `load()` is called, and drawing on a canvas does not
  load it, so `featureFamily` starts loading such a copy as soon as it is
  made.
- `featureFacesLoaded()` waits for the copies, as it already did where a
  face from bytes does not load at once. When a copy fails to load, it
  throws `fontFeatures cannot use "<family>": this browser did not load
  local("…") for it.` The face itself loaded, so the browser matched these
  names once; this covers a browser that limits `local()` differently for
  the copy. Safari matches only fonts that come with the operating system,
  so there the face itself already fails to load and the family is not
  loaded at all.
- Synchronous measurement (`useTextMetrics`) still needs `prepare()`: a
  `local()` copy is not ready in the task that makes it, so
  `featureFacesPending()` is true and `textMeasurer` asks for
  `await measureText(..., { fonts })` in `prepare()`, as for any copy that
  is still loading.
- The error for text with `fontFeatures` in a family no `<Font>` loaded now
  says the renderer cannot apply features to installed fonts "by their
  family name", since it can apply them to installed fonts a stylesheet
  names with `local()`.

## Remaining differences

- Text with `fontFeatures` in an installed family named directly, or with
  no `fontFamily`, still throws in the preview. The browser renderer has
  no face to copy; export applies the features.
- A copy that fails to load fails the frame only through
  `featureFacesLoaded()`. `featureFacesPending()` counts it as pending, so
  a synchronous measurement asks for `prepare()` first, where the error
  then names the fonts.

## Checks

```sh
pnpm --dir packages/web test
```

`packages/web/test/font-features.test.mts`: the `StubFontFace` now keeps a
string-source face `unloaded` until `load()`, then loads it a microtask
later, or fails it when every `local()` name is in `StubFontFace.missing`.

- `a local() face's copies name the same installed fonts, and measuring
  waits for them to load`: the copy's source is the face's `local()`
  string with its descriptors and `featureSettings`. It is `loading` right
  after `cssFont`, so a synchronous measurement throws, and measures after
  `loadFeatureFonts`.
- `a local() copy that does not load throws with the fonts it names`.
- The old assertion that a `local()` family throws is removed, and the
  "installed fonts" message is updated in the existing test.

## Screenshots

The PR's before and after images are one scene drawn by `SceneCanvas` in
the Claude desktop app's Chromium browser pane on macOS, from a page that
imports `@celesta/web`'s `dist/` built on the base branch and on this
branch. The scene's `<Font>` loads a stylesheet served as `text/css`:

```css
@font-face { font-family: Display; src: local(Missing-Font), local("Hoefler Text") }
```

It draws `office, fluffy, fjord` in `Display` twice, the second time with
`fontFeatures: { liga: false }`. Before, the preview throws for the second
line. After, it draws the second line without the ligatures.

A second scene in the same page measured `office` in `prepare()` with
`measureText(..., { fonts })`, in the engine's worker: 195.0 px with the
ligatures and 202.1 px without.
