# Plugin components in the editor's Add menu

The fourth layer of making content plugins usable from the desktop editor,
on top of `2026-11-02-plugins-panel.md`: a plugin can offer its components
in the Add menu, and the editor writes them with their import.

## Specification

- **Manifest.** `celesta-plugin.json` gains an optional `components` list
  (`ComponentInfo`). Old manifests stay valid, and unknown fields were
  already ignored, so older builds read new manifests too. Each entry:
  - `name`: the module's export, a capital letter then letters, digits, or
    `_`.
  - `label`: what the menu says; the name when absent.
  - `src`: `image` or `video` when the component takes a file as `src`.
  - `props`: what the component is added with. Names are identifiers;
    values are strings, numbers, or booleans. They may not set the props
    the editor writes from the drawing: `src`, `x`, `y`, `width` with `src`,
    else `x`, `y`, `width`, `height`.

  A manifest that lists components without a `module`, or breaks any of
  these rules, is skipped and reported like any invalid manifest.
- **The Add menu** (`AddMenu::components`, `AddComponent::offered`) lists the
  components of the plugins that may run (`EditorView::plugins`, so not
  waiting or turned-off ones), after Celesta's kinds and a separator, with
  the icon of the kind they are drawn as. They are disabled for the same
  reasons as every item (no parent, a timing or 3D problem). An import
  clash shows when the addition is written.
- **Adding.** Choosing one (`choose_add_component`) keeps it in
  `AddState::component` and runs the drawing of its kind
  (`AddComponent::kind`): Image or Video, which asks for and imports the
  file first, or Rect.
  - The button says **Adding Flag**.
  - On release the drawn element becomes the component
    (`AddComponent::element`): the drawn props, then the manifest's `props`.
  - `add_files` takes an `Addition` (Celesta's element or a plugin's
    component).
- **Writing.** `celesta_source_edit::add_component` writes
  `<Name attr={…} />` as the parent's last child, wrapped in a `Sequence`
  when timed, and imports `Name` from `@celesta-plugin/<id>`. An existing
  import of the module is reused under its name (an alias or a namespace).
  A local binding with the name locks the addition (`ImportLock::Clash`).
  `add_child` and `add_component` share the insertion (`add_written`), and
  the import step now takes any module, `@celesta/shapes` being one case.
- **The flag sample** lists `Flag` (`src: image`, `amplitude: 0.06`) in its
  `plugin.json`, and `build.mjs` copies `components` into the manifest.

## Implementation

| Where | What |
| --- | --- |
| `crates/plugin-host/src/manifest.rs` | `ComponentInfo`, `ComponentSource`, their checks in `invalid_reason`. |
| `crates/source-edit/src/add.rs` | `NewComponent`, `add_component`, `add_written`; `tags` by module. |
| `crates/editor/src/add_element.rs` | `AddComponent`, `Addition`, `AddMenu::components`, `AddState::component`, `choose_add_component`, `adding_name`. |
| `crates/editor/src/viewer.rs` | The menu items and the button's name. |
| `examples/plugins/flag/plugin.json`, `examples/plugins/build.mjs` | The sample's component. |

## Checks

- `cargo test -p celesta-plugin-host`: valid components and their drawn
  props, and manifests skipped for a lowercase or marked-up name, a prop
  that repeats a drawn one, an attribute-like or object prop, and
  components without a module.
- `cargo test -p celesta-source-edit --test add`: a component with every
  value kind, wrapped in a `Sequence`, reusing an aliased import, and a
  clashing binding. The existing additions are unchanged.
- `cargo test -p celesta-editor`: the flag component drawn as an Image and
  written with its import by `add_files`, and a box component's attributes.
- The editor run on Windows 11 with the rebuilt flag sample in a scratch
  user plugin folder:
  - the approval question said the plugin changed since it was allowed
    (its files had changed);
  - with a layer in a `<Group>` selected, **Add** listed **Flag** after
    Celesta's kinds;
  - choosing it asked for an image, the button read **Adding Flag**, and
    dragging a box wrote
    `<Flag src="./portrait.png" x={305.88} y={128.63} width={501.96} amplitude={0.06} />`
    with `import { Flag } from '@celesta-plugin/flag'`;
  - the preview drew the waving flag through the plugin's process.

  Screenshots before and after are on the pull request.
