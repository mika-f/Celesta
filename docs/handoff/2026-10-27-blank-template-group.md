# Blank starter wraps its layers in a Group (2026-10-27)

The editor's Add menu inserts only into a primitive `<Group>` (the selected
layer, or the nearest group around it). The blank starter was a `<Rect>` and
a `<Text>` directly under `<Composition>`, so a new project had nothing to
add into and every Add item stayed disabled ("Select a group, or a layer
inside one, to add into").

## What changed

`templates/blank/film.tsx` wraps the background and the title in a `<Group>`
with no transform, so the picture is unchanged and selecting either layer
makes that group the parent. The create-project pages (en, ja) and
`skills/celesta/references/setup.md` say so. An empty `<Group />` would not
do this: it draws nothing and cannot be selected.

## Checks

`templates/test/templates.test.ts` reads the starter and requires the `<Rect>`
and `<Text>` inside `<Group>`. The existing blank evaluation test is unchanged.
