Choose **File → Open…** to open a React composition (`.tsx`, `.jsx`, `.ts`, or `.js`). Select an asset, track, or clip to inspect its details.

> [!NOTE]
> **Your source is your canvas**
> Edit the composition in your text editor. Celesta previews and exports it; save the source to update the preview.

Compositions reload when you save. You can also choose **File → Reload** to reload manually.

**Keyboard shortcuts**

| Action | macOS | Windows / Linux |
| --- | --- | --- |
| Create a new project | `⌘ N` | `Ctrl N` |
| Open a project | `⌘ O` | `Ctrl O` |
| Reload the composition | `⌘ R` | `Ctrl R` |
| Play / pause | `Space` | `Space` |
| Step backward / forward | `←` / `→` | `←` / `→` |
| Jump backward / forward one second | `Shift ←` / `Shift →` | `Shift ←` / `Shift →` |
| Jump to the previous / next clip edge | `↑` / `↓` | `↑` / `↓` |
| Go to the start / end | `Home` / `End` | `Home` / `End` |
| Loop playback (the marked range if set) | `⌘ /` | `Ctrl /` |
| Toggle safe areas | `'` | `'` |
| Select layers (React compositions) | `V` | `V` |
| Zoom in / out / fit the timeline | `=` / `-` / `Shift Z` | `=` / `-` / `Shift Z` |
| Clear the export range | `Shift X` | `Shift X` |
| Mark export start / end | `I` / `O` | `I` / `O` |

While a field has the keyboard focus, these keys type or move the caret instead: the arrow keys, `Home`, and `End` never move the playhead, even at the start or end of the text, and neither do the matching items of the **Playback** menu. Click outside the field to use them again.

## Listen as you look

Playback synchronizes the picture and audio. Use waveforms and track levels to check timing, mute or solo tracks to isolate a sound, and adjust the preview volume. Drag along the timeline ruler to scrub to a specific moment, or swipe sideways over the tracks to move the playhead back and forth. Pinch over the tracks to zoom the timeline, and hold Option (Alt) while you scroll over them to pan it.

## Debug guides

`<DebugOverlay />` outlines a safe area, shades the canvas outside it, and marks the center point, and `<DebugBounds>` outlines a group of layers. They appear in the Celesta preview, and in PNG frames and contact sheets exported with `--debug-guides`, but never in a video, so you can leave them in while you work. `useDebugGuides()` tells your own components when guides are drawn.

## Find a layer's source

Turn on **Select layers** with the pointer button next to **Safe areas** below the viewer, **View → Select Layers**, or `V`. This interactive mode is available when you open a React composition. It records where each element was written and where each layer is drawn, which slows down scenes with thousands of layers, so the preview starts without it; turn it off again to play such scenes at full speed. Switching restarts the composition's runtime, which takes a moment.

In interactive mode, click a layer in the preview to select it. An orange outline marks it, and the **Properties** tab, next to **React Preview**, starts with a header that says what it is: an icon for the kind of element, its tag, such as `Rect`, and **in `<Title>`** when it is written inside one of your components. Below the tag, the element's position, such as `Title.tsx:12`, opens it in your text editor. The rest of the header tells you where the layer appears and what an edit of it changes, as described below.

The **Layer** section at the bottom of the tab shows the layer's ID. Expand its **Source chain** to see where the layer was written: **Source** gives the element nearest the layer, such as a `<Rect>` in `Title.tsx`, as its name and `file:line:column`, and **Called from** lists the elements around it, from the nearest outward, such as the `<Title>` that renders it and the `<Sequence>` it sits in. Click any row to open that position in your text editor. Click an empty part of the canvas to clear the selection.

A layer is hit anywhere inside the box it is drawn in, including transparent pixels of an image and the gaps between glyphs. A `<Rect>` with a stroke and no fill is hit only on its stroke, so you can select what is inside an outline. Where layers overlap, the one drawn last wins. A layer that is not in the frame being shown reads **Not in this frame**. A reload after an edit made outside Celesta, or turning interactive mode off, clears the selection. Celesta's own edits from the canvas, the property rows, or the props file's **Properties** section, and undo and redo, keep it. While the preview reloads after one of them, the tab keeps showing the layer with a small spinner in the header, and the property rows are read-only until the reloaded composition has drawn the layer again.

Celesta opens the position in the editor named by the `VISUAL` environment variable, then `EDITOR`. It can include arguments, such as `code -n`. On macOS and Linux, Celesta also asks your login shell for them when the app does not have them, such as when it was started from the Dock or Finder, so variables set in your shell profile still apply. VS Code (`code`), Cursor, Windsurf, Zed, Sublime Text, JetBrains IDEs, and TextMate open at the line and column; other editors are passed `+line file`. With neither variable set, the file opens in your system's default application, without a position.

Editors that run inside a terminal, such as `vim`, `nano`, and `emacs -nw`, cannot be started from the app. Celesta shows a message instead; set `VISUAL` to a graphical editor.

## See what an edit changes

An element written once can be drawn in many places: a component you use in several scenes draws its elements in each of them, and an element inside `.map()` is drawn once per item. Moving one of those layers moves the others too, so the header says where the selected layer appears and where else the same element is drawn. To know that, Celesta scans the whole composition frame by frame in the background, when you turn interactive mode on and after every reload while it is on. The scan runs in a separate runtime, so the preview stays responsive while it works; a long composition takes a while. After one of Celesta's own edits, the header and the timeline keep showing the last scan's answer until the new scan finishes.

Scenes are the chapters the composition declares with `chapters` on `<Composition>`. A part listed inside a scene counts as that scene, so an element in two parts of one scene is not shared. The current scene is the chapter at the playhead.

- **Appears in** lists the scenes the selected layer is on screen in. In a composition without chapters, it lists the time ranges instead, such as `00:00:02:00–00:00:04:00`; past the first three, the rest are counted, such as **and 2397 more**.
- **Shared** appears in the warning color when the same element is also drawn in other scenes, such as **also in Intro, Outro (3 instances)**. With more than three other scenes, it counts them instead, such as **also in 11 other scenes (71 instances)**. Each place your code renders the element, such as each `<Title>` you wrote, is one instance. Hover over the row to list the `file:line:column` each instance is rendered from, after the scene names when the row only counts them. While **Edits change** offers its options (see "Edit one scene or every scene"), the row stays and reads **this scene only** when the chosen option changes no other scene.
- **Can't move** lists the transform props that are locked and why, one reason per row, such as **X, Y — Written as an expression: W / 2**. The reasons are explained in the next section.

At the top of the panel, above the header, one line says what committing a change would do. It stays there while the header and the property sections scroll beneath it. For a shared element, a choice of what an edit changes takes its place, as described under "Edit one scene or every scene" below.

| Line | Meaning |
| --- | --- |
| Changes this scene only | The element is drawn in one place, and in no other scene |
| Changes `<count>` places in this scene | The element is drawn in more than one place in the current scene, such as by two `<Title>`s or in `.map()`, and in no other scene. The moments `<MotionBlur>` and `<Trail>` draw count as one place |
| Also changes `<scenes>` | The element is also drawn in the scenes named, shown in the warning color. With the playhead between chapters, every scene the element is in is named. More than three scenes are counted instead, such as **Also changes 11 other scenes**; hover over the line to list them |
| Scanning scenes… | The scan has not finished yet |
| Couldn't scan scenes: `<error>` | The composition failed on some frame during the scan; the rest of the preview works as before |
| Not found in the scene scan | The scan did not see this element, such as when it has no source position |

The line is shown for an element that is not shared: one with a single instance, or one written directly in a scene rather than inside one of your components. An element written inside one of your components with more than one instance, such as the `<Rect>` of `const PriceTag = ({ label }) => <Rect width={200} … />` used in Intro and Price, is shared. It shows **Edits change** with the options **This scene** and **Shared value** instead, and the **Shared** row and the timeline bands count the instances the option in effect changes.

On the timeline, the **Composition** row marks where the element is drawn: a filled band for the selected layer in the current scene, and an outlined band for each other place an edit of it also changes. Hover over a band to see its scene (or its timecodes in a composition without chapters), its frames, and where that instance is rendered from; click a band to jump to its first frame. Bands of one instance less than a pixel apart at the current zoom are drawn as one, whose tooltip says how many ranges it covers. In the **Chapters** row, the chapters the selected layer appears in are underlined. Until the scan finishes, a single filled band shows the frames the layer's sequences let it be drawn in. The filled band at the playhead also changes the element's timing, with grips or a **Set timing** button, and a chapter's end changes its scene's length, as described under "Change timing on the timeline" below.

## Move layers on the canvas

In interactive mode, a selected layer can be moved, rotated, resized, and scaled, and the result is written into your source. Drag inside the outline to move it, drag a square handle at a corner or an edge to resize it (or scale it, with the **Scale** handles), and drag the handle above the top edge to rotate it (hold Shift to snap to 15°). Rotation and scale turn about the layer's pivot unless you choose **Center**, as described at the end of this section. You can also type a value into the **Transform** rows below the header of the **Properties** tab and press Enter or leave the field; the next section describes every row of the tab. One drag or one committed field is one change, even when it writes two files: Celesta replaces only the number it changes and leaves the rest of your formatting and comments as they are, and the preview keeps the selection when it reloads.

Celesta writes a value only where your source states it plainly, on an element written directly in your file, or at the call of the component that element is written in:

- A number such as `x={80}` or `rotation={-12}` is replaced.
- A prop you left out is added, such as `x={140}` on a `<Rect>` with no `x`.
- A prop written as `interpolate(frame, [0, 30, 60], [100, 600, 300])` can be edited on a frame that is one of the listed keyframes, such as 30; the matching output value is changed. The frame is the value `frame` has where the element is written, so a `<Sequence>` between the `useCurrentFrame()` call and the element does not shift it. Between keyframes the field is read-only and shows **Between keyframes at 0 and 30**, with buttons that jump to them. Celesta does not insert keyframes.
- A `const x = 80` used by one element is replaced where it is declared.
- A prop of your component, such as `x` in `const PriceTag = ({ x }) => <Rect x={x} />`, is written where the component is called, such as `<PriceTag x={40} />`, by the rules above. When the call leaves the prop out and the parameter has a default (`{ x = 0 }`), the prop is added to the call. Renamed props (`{ x: left }`) and `props.x` are followed too.

A row whose value is set at a component's call site shows an arrow and **Set at `<file>`:`<line>`** under its field, such as **Set at scenes/price.tsx:12**, with the file relative to the entry file's folder. Dragging the layer or typing into the row writes that call, which may be in another file than the element. One drag can write both files, such as a call site's `x` and a `rotation` written on the element; it is still one change, and one undo restores both files. Because each call is edited on its own, an element in a component you call in several places is not locked as rendered more than once when its value is set at the call. A value passed at a call inside `.map()` is editable too, and changes every copy that call renders.

Anything else is locked: its field shows a lock icon and the reason, the header's **Can't move** rows list it, and the outline turns dashed when nothing on the layer can be moved. The reasons are below; a word in angle brackets, such as `<name>`, stands for the part taken from your source.

| Reason | Meaning |
| --- | --- |
| Set by a component, not written on this element | The element nearest the layer is your own component (or another library's), not a Celesta element such as `<Rect>` |
| Written as an expression: `<expression>` | A value such as `W / 2` or `i * 24`, or a value of another kind than the row takes, such as `anchorY="baseline"` in the **Anchor Y** row |
| Comes from `<name>`, which can't be traced | An import, a `let`, a parameter of an outer component read inside a nested one, an object property, or a name declared more than once in the component, such as again inside a block or loop |
| Comes from `<component>`'s caller | The call site passes on a prop it received itself, such as `<PriceTag x={x} />` inside `Card`. Celesta follows one call, not the call of that call |
| Can't find where `<component>` is called | The value is a prop of the component, but Celesta can't find the call that renders this layer, or can't read its file |
| `<name>` is used in `<count>` places | Editing the shared `const` would move other elements too |
| Between keyframes at `<frame>` and `<frame>` | The frame is not one of the keyframes; use the buttons to jump to one |
| Outside the keyframes | The frame is before the first or after the last keyframe |
| Computed by `<function>`() | A call such as `spring()` |
| A layout component sets this value | The element received another value than the one written: a parent, such as a layout component, replaced it |
| May be set by a spread ({...props}) | The element has `{...props}` and no value of its own, or the spread comes after the value |
| May be set by `style` | A font or color row of a `<Text>` whose `style` is not written as an object, or has a spread that could set the value (see the next section) |
| Rendered `<count>` times from this element | An element in `.map()`, or in one of your components drawn more than once in the frame; editing it would move every copy. For a component, **This scene** can move the selected copy alone when the call that renders it draws it alone (see "Edit one scene or every scene") |
| Can't tell which frame this animation reads | `frame` could hold more than one frame (each `<Sequence>` or `<FreezeFrame>` around the element gives another), and those frames give different keyframes with the same value |
| This file is outside the project | The value is written in a file outside the entry file's folder, or inside `node_modules`: the element's file, or the call site's for a value set there |
| Couldn't find this element in the source | The file no longer has the element where the preview found it, such as while a save is still reloading |
| The source has syntax errors | Fix the file first; every field is locked until it parses |

If a file the drag would write changes while you drag, Celesta discards the drag and says so instead of writing over your edit. Dragging on the canvas edits `x`, `y`, `rotation`, and `scale` (or `scaleX` and `scaleY`, when written), the props that hold the layer's size, such as `width`, and `anchorX` and `anchorY`, as described at the end of this section. Skew is not edited.

A layer inside a `<Group>` or `<Stack>` moves with its parent. A child of a `<Stack>` stays editable: its `x` and `y` are offsets from the place the `<Stack>` gives it. Under **Source chain**, each row of **Called from** that is a Celesta element drawing its own layer, such as a `<Group>`, has a **Select** button that selects that layer; a group is outlined as the box around its children, and dragging anywhere inside it moves the whole group. A `<Stack>` or `<SafeArea>` row has no **Select** button: they are components, and the groups they create are not written in your source.

### Turn about the center or the pivot

While **Select layers** is on and a layer is selected, a row of tools sits at the top of the viewer, above the frame (which makes room for it), with two labelled toggles; in each, the choice you made is the raised tab. The first, **Rotate about** [**Center** | **Pivot**], sets what the rotation handle and the **Scale** handles turn about; a cross on the selected layer marks that point:

- **Pivot**, the default, turns the layer about its pivot, the point the renderer turns it about: its anchor (`anchorX` and `anchorY` of its box, the top-left corner unless you set them), moved by `originX` and `originY` when you set those. Only `rotation` or the scale is written. The cross sits on the pivot, and Alt-dragging it moves the anchor (see "Move the anchor" below).
- **Center** keeps the middle of the layer's box where it is. To do that, Celesta also writes `x` and `y`: a rotation writes `rotation`, `x`, and `y` together, as one change, which writes two files when `x` and `y` are passed through from a component call. The cross sits on the middle of the box. It only marks that point: its tooltip says **Rotates about the center; switch to Pivot to move the anchor** (or, when the anchor couldn't move in **Pivot** either, **Rotates about the center; the anchor can't move:** and the reason, as listed under "Move the anchor"; for an element without anchor props, such as a `<Path>`, just **Rotates about the center**). Holding Alt over it and pressing does nothing.

**Center** needs `x` and `y` that Celesta can write: each a number, a prop left out, or a value passed through from a component call, as described above. It also needs a `rotation`, scale, size, and anchor that are not animated with keyframes: `x` and `y` written on one keyframe would keep the middle still on that keyframe only, and the layer would drift on the others. A layer that doesn't meet these turns about its pivot even with **Center** chosen, and the cross sits on the pivot. The toggle then keeps **Center** chosen, so the next layer you select uses it again, and shows a warning icon whose tooltip gives the reason:

| Tooltip | Meaning |
| --- | --- |
| x or y can't be edited | `x` or `y` is animated with keyframes or locked, such as a value computed in your code |
| Rotation or scale is animated | `rotation` or a scale is animated with keyframes |
| Size or anchor is animated | A size prop, `anchorX`, or `anchorY` is animated with keyframes |
| No flat box to center on | The layer is turned in its own perspective |

The same reason, after **Pivot —**, shows when you hover over the rotation handle or a **Scale** handle, and beside the layer while you drag one.

### Resize with the Size handles

The second toggle, **Handles** [**Size** | **Scale**], sets what the corner and edge handles do. With **Size**, the default, they change the props that hold the layer's size, and the edge or corner opposite the one you drag stays where it is, whichever of **Center** and **Pivot** is chosen:

| Element | Handles | Writes |
| --- | --- | --- |
| `<Rect>`, `<Image>`, `<Ellipse>`, `<Triangle>`, `<Heart>` | Four corners and four edges: an edge changes one side, a corner both | `width`, `height` |
| `<Circle>`, `<Polygon>`, `<Star>`, `<Pie>` | Four corners, which resize evenly | `radius` |
| `<Text>` | The left and right edges | `maxWidth` |

The handles sit on the box those props describe, which is drawn as the selection outline, and follow the layer's rotation, scale, and skew; a drag is measured along the layer's own axes. Hold Shift while you drag a corner to keep the box's proportions. A `<Text>`'s box is its layout box: dragging an edge starts from its `maxWidth` when set, otherwise from the width of its longest line, wraps the text to the new width, writes `maxWidth` in whole pixels, and adds it when it is left out. An `<Image>` that leaves out `width` or `height` sizes it from the picture's proportions; dragging the other side writes it at its current size, so only the side you drag changes.

When the edge that stays is not where the anchor is, such as the left and top edges of a layer anchored at its top-left corner, Celesta also writes `x` and `y` to keep it there. That needs `x` and `y` that Celesta can write, as for **Center**, and a `rotation`, scale, and anchor that are not animated with keyframes; otherwise those handles are hidden. A size stops at its prop's range, such as 1 for an image's **Width**, and never goes below 1 pixel, so dragging an edge past the opposite one does not flip the box. A size animated with keyframes can be resized on one of its keyframes, as in its row. The handles that would move the anchor along that size's direction are hidden, since the `x` and `y` they write would keep the edge still on that keyframe only. The handles that don't move the anchor along that axis still work: for a `width` animated on a layer anchored at its top-left corner, the left edge and the two left corners are hidden, and the right edge, the top and bottom edges, and the two right corners work.

An element without size props, such as a `<Group>`, a `<Video>`, or a `<Path>`, or one whose size can't be written, gets the **Scale** handles instead. The toggle keeps **Size** chosen for the next layer and shows a warning icon whose tooltip gives the reason:

| Tooltip | Meaning |
| --- | --- |
| No size to resize | The element has no size props, or the preview didn't report its size |
| `<Row>`: `<reason>` | A size row is locked, such as **Height: Written as an expression: h * 2**; the reasons are listed above |
| Size can't be edited here | A size prop has no row Celesta can write |
| x or y can't move | Every handle would move the anchor, and `x` or `y` is animated with keyframes or locked |
| Rotation or scale is animated | Every handle would move the anchor, and `rotation` or a scale is animated with keyframes |
| Anchor is animated | Every handle would move the anchor, and `anchorX` or `anchorY` is animated with keyframes |
| Size is animated | Every handle would move the anchor along a size animated with keyframes |
| No flat box to resize | The layer is turned in its own perspective |

With **Scale**, the corner and edge handles scale the layer, writing `scale`, or `scaleX` and `scaleY` when your source writes either. The rotation handle works the same in both tools.

### Move the anchor

With **Pivot** in effect, the cross marks the pivot of the selected layer. Hold Alt (Option) and drag the cross to move the anchor: Celesta writes `anchorX` and `anchorY` as fractions of the layer's box (0 at its left or top edge, 1 at its right or bottom; a value outside the box is kept), and moves `x` and `y` with the cross, so the layer stays where it is on screen. The box is the one the anchor refers to: a `<Text>`'s layout box, a shape's `width` × `height` (twice its `radius` for a `<Circle>`), or an image's or video's picture. While Alt is held, the pointer over the cross is an open hand, and the cross takes the press even when it sits on a handle. Like **Center**, moving the anchor needs `x` and `y` that Celesta can write, and an anchor, `rotation`, scale, and size that are not animated with keyframes. Rotation then turns about the new anchor. With **Center** in effect, the cross marks the middle of the box instead and does not move the anchor; choose **Pivot** to move it.

When the cross can't be dragged, the pointer over it shows a not-allowed sign while Alt is held, and hovering over it shows the reason:

| Tooltip | Meaning |
| --- | --- |
| `<Row>`: `<reason>` | The row's own lock reason, such as **X: Comes from DX, which can't be traced** for an `x` set from an imported constant, or **Anchor Y: Written as an expression: "baseline"** for a `<Text>` with `anchorY="baseline"` |
| X is animated, Y is animated | `x` or `y` is animated with keyframes |
| Anchor is animated | `anchorX` or `anchorY` is animated with keyframes |
| Rotation or scale is animated | `rotation` or a scale is animated with keyframes |
| Size is animated | A size prop is animated with keyframes |
| A group's anchor has no effect | A `<Group>` places its children by its position alone |
| No box to anchor in | The layer is turned in its own perspective, or the preview didn't report its size |

A `<Path>`, `<Line>`, `<Arrow>`, and other elements without anchor props have no anchor to move.

## Edit properties

Below the header, the **Properties** tab lists the selected element's props in sections, one row per prop, in the order the element declares them. Every Celesta element starts with **Transform**: X, Y, Rotation, Scale (Scale X and Scale Y instead when your source writes either), Anchor X, Anchor Y, and Opacity. The sections after it depend on the element:

| Element | Sections after Transform |
| --- | --- |
| `<Rect>` | **Size** (Width, Height, Corner radius), **Appearance** (Fill, Stroke, Stroke width) |
| `<Text>` | **Text** (Text, Font family, Font size, Font weight, Color), **Layout** (Max width) |
| `<Image>` | **Source** (Source), **Size** (Width, Height, Fit) |
| `<Video>` | **Source** (Source, Start from, Playback rate, Loop), **Audio** (Volume, Muted) |
| `<Group>` | — (**Transform** only) |
| `<Ellipse>`, `<Heart>` | **Size** (Width, Height), **Appearance** (Fill, Stroke, Stroke width) |
| `<Circle>` | **Size** (Radius), **Appearance** as for `<Ellipse>` |
| `<Triangle>` | **Size** (Width, Height, Apex, Corner radius), **Appearance** as for `<Ellipse>` |
| `<Polygon>` | **Size** (Radius, Sides, Corner radius), **Appearance** as for `<Ellipse>` |
| `<Star>` | **Size** (Radius, Points, Inner ratio, Corner radius), **Appearance** as for `<Ellipse>` |
| `<Pie>` | **Size** (Radius, Start angle, End angle, Corner radius), **Appearance** as for `<Ellipse>` |

Other elements, such as `<Path>`, `<Line>`, `<Arrow>`, and `<Mask>`, show only X, Y, Rotation, and Scale (or Scale X and Scale Y) under **Transform**.

Each row writes its prop where your source states it plainly, by the rules of the previous section: on the element, at the call of your component (the row then shows **Set at `<file>`:`<line>`**), or in a `const` used once. Strings, `true` and `false`, and the text of a `<Text>` are written like numbers, and a string keeps the quotes it was written with. Anything else is locked with its reason, as on the canvas. A prop your source leaves out shows, dimmed, the value it is drawn with: the default of your component's parameter when the prop is one, otherwise the value the layer received, or the element's own default. Entering a value adds the prop.

Each row has a control for its kind of value:

| Value | Control | Written when |
| --- | --- | --- |
| Number | A field; drag the row's label to scrub it | You press Enter or leave the field, or release the label |
| Text, the **Text** row of a `<Text>` | A field that grows to several lines | You press Enter or leave the field; Shift+Enter starts a new line |
| String, such as **Font family** | A field | You press Enter or leave the field |
| On or off, such as **Loop** | A switch | At once |
| One of a list, such as **Fit** | A menu | At once |
| Color | A field and a swatch that opens a picker | You press Enter or leave the field, or close the picker |
| File, such as **Source** | A field with **Choose…** and **None** | You press Enter, leave the field, or choose a file |

Escape in a field puts the source's value back. A value the row can't take, such as a word in a number row or a number outside the prop's range, shows a message under the row and is not written. **Opacity** takes 0 to 1 and **Font weight** 1 to 1000; sizes, radii, stroke widths, **Font size**, **Max width**, and a video's **Start from**, **Playback rate**, and **Volume** take 0 or more; an image's **Width** and **Height** take 1 or more, a shape's **Stroke width** 0.1 or more, and **Sides** and **Points** take whole numbers of 3 or more. A color is `#RRGGBB`, `#RRGGBBAA`, or the name of a color of your `<Theme>`, such as `accent`, written as you type it; a name shows no swatch. A menu whose source holds a value that is not one of its choices, such as `fit="fill"`, shows that value until you choose another. **Choose…** writes the file's path relative to the entry file's folder, copying a file from outside that folder into its `assets` folder first. **None** writes an empty path over a written one, and nothing when the prop is left out.

To remove a prop your source writes, so its default applies again, empty its field and press Enter (or leave the field), or hover the row and click the **×** at its end; switches and menus have the **×** too. The attribute goes from the element, a property from its `style={{ … }}`, or, in a row with **Set at**, the attribute from the call, so your component's default applies. The row then shows, dimmed, the value drawn without it. Props an element can't do without, such as a `<Rect>`'s **Width** and **Height**, an `<Image>`'s **Source**, or the **Text** of a `<Text>`, have no **×**, and emptying their field puts the value back, unless the value is set at a call whose parameter has a default written as a plain value, such as `rectWidth={260}` passed to `{ rectWidth = 200 }`: removing it there lets the default apply. So do the props your component's `registerComponent()` schema marks `required: true`, whether they show in the **Component** section or in a row with **Set at**; a component without a schema can have any of its props removed. The same goes for props your source leaves out, locked rows (except one locked because the current scene sets its own value, see "Edit one scene or every scene"), rows animated with keyframes, and input rows. For a shared element, what the **×** removes follows the option chosen at the top of the panel, as described under "Edit one scene or every scene".

While Celesta reloads after writing a value, the rows stay editable: the field you typed in keeps the keyboard focus, you can go on typing, and a value you commit meanwhile is written once the reload finishes; until then, its row says **Saving…**. Leaving a field commits it, also when you leave it by selecting another layer, clicking an empty part of the preview, moving the playhead, or starting playback: what you typed is written as if you had pressed Enter, and text the row can't take is dropped. When a value is still waiting for the reload, the new selection shows once that value is written. Turning interactive mode off waits the same way: until the value is written, the **Select layers** button shows a spinner and the layer stays selected, and turning the mode on again meanwhile keeps it on with the selection. If the code fails to reload, the waiting values are not written, and their rows say **Not saved: the code failed to reload**. If the preview stops unexpectedly, they say **Not saved: the preview stopped unexpectedly**, and no value is written from the rows until you restart Celesta.

To scrub a number, drag its label left or right. Each pixel changes the value by the prop's step: 0.01 for **Scale**, **Scale X**, **Scale Y**, **Anchor X**, **Anchor Y**, and a video's **Playback rate** and **Volume**, 100 for **Font weight** (a font's faces come in weights 100 apart), and for a component prop, the `step` its `registerComponent()` schema declares. A prop without a step changes by 0.01 when its range ends at 1 or below, such as **Opacity**, and by 1 otherwise. Holding Shift makes each step ten times larger. A value stops at the prop's range. A prop your source leaves out starts from the dimmed value its row shows. The preview follows while you drag, including the other layers drawn from the same value, and the value is written once, when you release the mouse. Press Escape before releasing to put everything back without writing; releasing without moving, or with the value back where it started, writes nothing either. Text you typed in the row you scrub gives way to the scrubbed value. Text you typed but haven't committed in another row stays in its field, which keeps the keyboard focus; otherwise pressing a label moves the focus out of the fields, so `⌘ Z` (`Ctrl Z`) right after a scrub undoes it. Rows that are locked or animated with keyframes don't scrub, neither do input rows, and labels don't scrub during playback. Dragging a layer on the canvas and scrubbing a label never run together: starting one ends the other without writing.

A value computed from others, such as `width={textWidth + padding * 2}`, stays locked, and the names it reads are listed below its row as indented input rows, such as **textWidth** and **padding**. Each input is traced by the same rules, so a `const` used once, or a prop of your component set at the call (with its own **Set at** line), can be edited there; the others show their lock reason. Inputs are one level deep: an input that is itself an expression is not expanded. A value computed at your component's call, such as `<PriceTag x={left + 8} />`, lists no inputs.

The **Font family**, **Font size**, **Font weight**, and **Color** rows of a `<Text>` follow its `style`, since `style` wins over those props. When `style={{ … }}` sets the value (`fill` for **Color**), the row edits it there. Otherwise the row edits the prop itself, adding `fontSize={48}` beside a `style` that doesn't set it. When `style` is not written as an object, such as `style={mono(20)}`, or a spread in it or on the element could set the value, the row is locked with **May be set by `style`**.

When the element is written inside one of your components and Celesta finds the call that renders it, a **Component** section follows, such as **Component · PriceTag**, with **Set at `<file>`:`<line>`** for that call. It lists the props written at the call in the order they are written, so you can change `label="Sale"` on `<PriceTag>` while a `<Rect>` inside it is selected. A prop a row above already shows with **Set at** is not repeated, and `key`, `ref`, spreads, and children are left out. A number gets a number field, a string a text field, and `true` or `false` a switch. When the component is registered with `registerComponent()`, its schema gives the control and the range instead, a field's `label` names its row (a prop the schema leaves out shows its name as written), a number field with `integer: true` takes whole numbers only, a field with `required: true` can't be removed, and a value of another kind than the schema's is locked. When the schema is typed with your props, as in `ComponentPropertySchema<BadgeProps>`, TypeScript makes you mark each prop the props type requires `required: true`, and only those. A value that is not written plainly is locked with its reason.

**Can't move** looks at the transform props only. A row of the **Component** section always writes its call, so it changes only the instances that call renders. For a shared element, every other row, a scrub, a drag on the canvas, and a text edited on the canvas write under the option chosen at the top of the panel, as described under "Edit one scene or every scene".

## Edit text on the canvas

In interactive mode, double-click a `<Text>` in the preview to edit its text where it is drawn, in its own font, size, colors, and wrapping, also when it is rotated, mirrored, or tilted in 3D. A text that isn't selected yet is selected by the first click, and the edit begins as soon as Celesta has read its source. Playback pauses. While you edit, the handles and the cross go and the outline stays; a steady caret in the accent color marks where you type, and a selection is shaded in it.

The preview redraws the text with every key you press, but nothing is written until you commit, and your code is not run meanwhile: what it computes from the text, such as a frame sized to it, keeps its old size until the commit's reload. A text revealed with `visibleCharacters` shows in full while you edit; the reload brings the reveal back.

| Key | What it does |
| --- | --- |
| Arrows, `Home` / `End` | Move the caret; with Shift, select. Up and down keep the caret's column across a shorter line |
| `⌘ A`, `⌘ C`, `⌘ X`, `⌘ V` (`Ctrl` on Windows and Linux) | Select all, copy, cut, and paste. Pasted text is plain, and its line breaks become ordinary ones |
| `Shift Enter` | Start a new line |
| `Enter`, `⌘ Enter` | Commit the edit |
| `Escape` | Leave without writing; the preview shows your source's text again |
| `Tab`, `⌘ Z`, `⌘ Shift Z` | Nothing while you edit; after the commit, one undo restores the whole edit |

While you edit, the keys go to the text: `Space`, `V`, `I`, `O`, and the arrows type or move the caret instead of playing, seeking, or switching modes. **File → New**, **Open…**, **Reload**, **Close**, and **Export** (and their shortcuts) commit the edit first, and **Reload**, **Close**, and quitting wait until it is written, for up to five seconds; `⌘ /` (loop) and **Show problems** leave it running.

Click inside the text to move the caret, drag to select, and Shift-click to extend the selection. A click outside the text's layout box commits the edit and then acts as usual. On a single line without `maxWidth`, the box ends at the last letter, so a click just past it is outside. A click anywhere else in the window, such as in the **Properties** tab or the tool row, commits the edit too, and the keyboard focus stays in what you clicked; the tab's **Text** row stays editable and shows the written text after the commit. Selecting another layer, clearing the selection, turning interactive mode off, moving the playhead, and starting playback also commit.

Japanese and other languages entered through an input method work as in a text field: the text being composed is drawn in the preview, underlined, and the candidate window opens at the caret. While composing, the first Escape goes to the input method and cancels the composition; the next one leaves the edit. Ending the edit in the middle of a composition, such as by clicking outside the text, writes what is shown.

A commit is written the way the **Text** row writes it: into the JSX text or the string where your source states it, into a `const` used once, or at your component's call for a text passed through a prop, such as `<Label text="Sale" />`. A text with line breaks is written as a string, such as `{"Sale\nToday"}`. It is one change, and one undo restores it. A text left as it was, or emptied, writes nothing (a `<Text>` needs its text, so emptying it puts the source's back). The preview keeps the new text after the commit, without flashing the old one, until the reload draws it. When another value is still being written, the commit waits its turn, as a row's does. If the write fails, such as when the file changed since you began, the preview shows the source's text again. The reason shows at the top-left of the preview, until your next click, and in the **Properties** tab.

A double-click doesn't begin an edit on a text Celesta can't write. It shows the reason beside the layer, styled like the label of a drag, until your next click:

| Note | Meaning |
| --- | --- |
| The **Text** row's lock reason, such as **Written as an expression: a + b** | The text is not written plainly in your source, such as a value computed in your code or `<Span>` children; the reasons are listed under "Move layers on the canvas" |
| Why the layer's source couldn't be read, such as **Not in this frame** | As the **Layer** section says |
| The code is reloading | The composition is reloading; double-click again when it has finished |
| The text isn't drawn in this frame | The text is not in the frame shown, or a corner of it is behind the viewer |
| The text is edge-on to the view | The text is turned so far in 3D that it has no area to type in |
| The text can't be edited here | The **Text** row has nothing Celesta can write |

A double-click on a text you have just committed waits while the commit is written, then shows **The code is reloading** while the composition reloads; once it has, double-click again to edit the new text. If your file changes outside Celesta while you edit, the edit ends without writing, and **The source changed while editing** shows at the top-left of the preview until your next click.

## Edit one scene or every scene

An element written inside one of your components is drawn by every call of it. When the selected element has more than one instance, such as the `<Rect>` of a `<PriceTag>` you call in the Intro, Price, and Outro scenes, you choose before you commit whether an edit changes this scene or the value the scenes share. The line at the top of the panel becomes the choice, and stays in view while you scroll the panel, so you can see and switch it while editing rows further down:

```text
Edits change  [ This scene ]  [ Shared value ]
              Shared value is used in Intro, Outro · Price sets its own
```

- **This scene**, the default, writes the edit for the instances in the current scene only.
- **Shared value** writes the value every scene uses unless it sets its own.

The chosen option is the raised tab, as in the tool row at the top of the viewer. Click the other one to choose it. Celesta remembers the choice for that element for the rest of the session, so editing a background again doesn't ask again. After one of Celesta's own edits, the element you are editing keeps its choice even when the edit moves it, such as one that adds a line above it; the other elements in the files that edit wrote may have moved too, so they go back to the default. When your files change outside Celesta, every element goes back to the default. While the pointer is over a tab, the timeline bands and the header's **Shared** row show the instances that option would change; otherwise they show the instances the chosen option changes.

The line under the tabs names the scenes the shared value reaches and the scenes that set their own value, which a **Shared value** edit leaves as they are: **Shared value is used in Intro, Outro · Price sets its own**. A scene sets its own when its call of the component passes any of the props the element's rows read. Several such scenes read **Price, Sale set their own**, and more than three are counted, such as **5 scenes**. In a composition without chapters, the line reads **Shared value is used everywhere**; when every scene sets its own, it reads **Shared value is used in no scene**.

When the current scene calls the component more than once, such as a Sale and a Second `<PriceTag>` in Price, **This scene** writes the call of the instance you selected only, and the other calls in the scene keep their values. While **This scene** is in effect, or the pointer is over it, the line reads **Edits this `<PriceTag>` only · this scene calls it 2 times**, and the timeline bands show the selected instance alone. The tab's tooltip reads **Write the edit for this call only**. In a composition without chapters, where the whole timeline is one scene, the line and the tooltip stay as for one call.

### What This scene writes

A value your component receives as a prop, shown with **Set at** in its row, is written at the current scene's call, as without the choice. Any other value is moved into a new prop of the component: its current value becomes the prop's default, so the other scenes keep it, and the new value is passed at this scene's call. Choosing **This scene** in the Price scene and setting the width to 260:

```tsx
// before
function PriceTag({ label }: Props) {
  return <Rect width={200} … />;
}
// Price.tsx
<PriceTag label="Sale" />

// after
function PriceTag({ label, rectWidth = 200 }: Props) {
  return <Rect width={rectWidth} … />;
}
// Price.tsx
<PriceTag label="Sale" rectWidth={260} />
```

The new prop is named after the element's tag and the prop, such as `rectWidth`, `textFontSize`, or `rectX`; when your component's file already uses that name, or the scene's call already writes it (passed on through `...rest`), a number is added, such as `rectWidth2`. The default is written the way the value was, such as `"200"` for `width="200"`. A component without a parameter gets one (`function PriceTag({ rectWidth = 200 })`), and a parameter with `...rest` gets the new prop before it. When the parameter's type is a type literal, or an `interface` or `type` declared in the same file (also through `FC<Props>`, `memo<Props>`, or `forwardRef<…, Props>`), it gains an optional field, such as `rectWidth?: number`. A prop the element leaves out is moved the same way, with its default when Celesta knows it, and a `const` used once inside the component takes the new prop as its value. Moving the layer writes `x` and `y` together. The edit writes two files, often, and is still one change; one undo restores both.

After that, editing the same element in another scene writes that scene's call, such as `rectWidth={180}` on the Intro's `<PriceTag>`, and the component stays as it is.

When the scene calls the component more than once, the value is passed at the selected instance's call only; the scene's other calls leave the new prop out, so they keep the old value as its default. A value written on the element of a component the frame draws more than once, such as the `x` of both price tags, is locked with **Rendered 2 times from this element** under **Shared value**, since writing it there would move every copy. Under **This scene**, when the selected instance's call draws it alone, it is moved into a new prop like any other value, so you can drag one of the tags on the canvas: `x` and `y` go to its call, and the other tag stays where it is. While you drag, or scrub the **X** or **Y** label, the preview draws every copy at the new place; once you release, only the selected call is written, and only the selected copy moves. When that call draws the element more than once itself, such as in `.map()`, the value stays locked.

### What Shared value writes

A value written on the element is written there, as without the choice: every scene changes, including one that sets its own value for another prop. For a prop of your component that the current scene's call leaves out, the row shows and edits the parameter's default, such as `rectWidth = 200` in `{ label, rectWidth = 200 }`, without **Set at**; a parameter without a default gets one, such as `{ label, rectWidth = 240 }`, and its row shows the element's default, dimmed. The calls that pass the prop keep their value, and the line names their scenes. Only such a value spares the scenes that set their own: when every row of the element is a prop of your component, the timeline bands and the header's **Shared** row leave those scenes out; otherwise they show every instance.

When the current scene's call passes the prop itself, such as `rectWidth={260}` on Price's `<PriceTag>`, an edit of the shared value would change the other scenes but not this one. The row shows this scene's value, the one the preview draws, with its **Set at** line naming the call, and is locked with **This scene sets its own `rectWidth`**; dragging the layer, scrubbing the label, and editing the text on the canvas are locked for that prop too. When the call writes its value plainly, such as `rectWidth={260}`, you can edit the shared value from this scene: hover the row and click its **×**, whose tooltip reads **Use the shared value**. It removes the attribute from the call, and the row then shows the shared value and edits it. A value the call computes, such as `rectWidth={size * 2}`, has no **×**, nor does a prop the element can't do without whose parameter has no default written as a plain value; edit the shared value from a scene whose call leaves the prop out. To change this scene's value instead, choose **This scene**.

### When an option can't be chosen

An option that can't write the edit is dimmed; the line under the tabs and the tab's tooltip give the reason. When **This scene** is disabled, **Shared value** is chosen instead. The reasons below are shown for the `<Rect>` of `PriceTag` and its `rectWidth`; Celesta names your component and prop. **This scene** is disabled when:

| Reason | Meaning |
| --- | --- |
| Can't find where `<PriceTag>` is called | Celesta can't find the call that renders this scene's instance, or can't read its file |
| Passes through `<Card>` | Another of your components, here `<Card>` around `<PriceTag>`, sits between the scene and the component the element is written in. Scenes written as components and placed by hand, such as `<Sequence><PriceScene /></Sequence>`, count as one too |
| The scenes reuse the same call (`.map()` or `defineScenes`) | One call renders the component for several scenes, such as scenes made by `defineScenes` and drawn by one `<Scenes>` |
| `<PriceTag>` is called here for other scenes too | The call that renders this scene's instance also renders instances in other scenes, such as a call inside `.map()` |
| `<PriceTag>` here draws this element more than once | This scene calls the component more than once, and the selected instance's call draws the element more than once itself, such as a call inside `.map()`; writing that call would change every copy it draws |

The next reasons belong to a row. Under **This scene**, the row is locked with its reason, and the option is disabled only when no row can be written:

| Reason | Meaning |
| --- | --- |
| `PriceTag` doesn't take its props as `{ … }` | The component's parameter is written as `(props)` or `(...args)` |
| `PriceTag`'s props type is in another file | The parameter's type is imported |
| Can't add `rectWidth` to `PriceTag`'s props type | The type has no type literal to add the field to, such as `type Props = Base & Other` |
| Can't be set per scene yet | The value is animated with keyframes, is in a `style={{ … }}` object, or is an input row's `const` |
| This file is outside the project | The component's file or the call's file is outside the entry file's folder, or inside `node_modules` |

Under **Shared value**, a row is locked, and the option disabled when every row is, except by **This scene sets its own**, which locks the row but never the option:

| Reason | Meaning |
| --- | --- |
| Every scene sets `rectWidth` itself | The parameter has no default, and every call passes the prop |
| This scene sets its own `rectWidth` | The current scene's call passes the prop; the row shows this scene's value. When the call writes the value plainly, the row's **×** removes it from the call; otherwise, edit the shared value from a scene whose call leaves the prop out |
| `rectWidth`'s shared value is written as an expression | The default is computed, such as `{ rectWidth = W * 2 }` |
| `PriceTag` doesn't take its props as `{ … }` | The component reads `props.rectWidth`, which has no default to write |
| Can't find where `rectWidth`'s shared value is written | Celesta can't find the parameter in the component |
| This file is outside the project | The component's file is outside the entry file's folder, or inside `node_modules` |

If the option you chose is disabled, such as when you chose **This scene** in Price and select the element in a scene whose call also renders other scenes, Celesta doesn't switch to the other one: neither tab is raised, the line shows the reason in the warning color, and edits are refused with it until you choose an option.

### Removing a value

Under **This scene**, a row whose value is passed at the current scene's call has the **×**, whose tooltip reads **Use the shared value**: it removes the attribute from the call, so the scene uses the shared value again. A value written on the element has no **×** under **This scene**, since removing it would reach every scene, and emptying its field puts the value back. Under **Shared value**, a value written on the element is removed as usual, a row this scene sets itself, with the value written plainly at the call, has the **×** with **Use the shared value** too, which removes the attribute from the call, and a parameter's default has no **×**. The **Component** section is outside the choice: its rows write and remove at the call, as before.

### Dragging, scrubbing, and editing text

While you drag a layer, the label beside it has a second line, **Edits change this scene** (**Edits change this `<PriceTag>` only** when the scene calls the component more than once) or **Edits change the shared value**, and the header says the same under the element's name. A drag, a scrub, and a text edited on the canvas write under the option in effect when they began. One that this option would refuse doesn't begin: the canvas offers no handle for a prop it can't write, and no size handles when it can't write one of the layer's size props, and a scrub or a double-click shows the reason instead, as a locked row does. If that option is disabled by the time you release or commit, the edit is refused with the reason instead of being written under the other one. A refused text edit stays open, with the reason at the top-left of the preview, so you can choose an option and press Enter again; selecting another layer, moving the playhead, starting playback, or a **File** command ends it without writing.

### While the scenes are scanned

Until the scan described under "See what an edit changes" answers, **Edits change** is followed by its state instead of the tabs:

| Line | Meaning |
| --- | --- |
| Scanning scenes… | The scan has not finished yet |
| Couldn't scan scenes: `<error>` | The composition failed on some frame during the scan |
| Not found in the scene scan | The scan did not see this element |
| The scenes are not scanned | No scan runs for the composition |

Meanwhile, a value set at the call, in a row with **Set at** or in the **Component** section, is written at the call, as without the choice, unless you chose **Shared value** for the element earlier. Any other value committed in a row waits while the scan runs, with **Scanning scenes…** under the row, and is written when the scan finishes. If you chose an option for the element before, it is written under that option. Otherwise you didn't see the options when you committed it, so it is written only if the option in effect when the scan finishes is **This scene**; when **This scene** is disabled by then, it is not written to the shared value but refused with **The scenes were scanned while editing; edit again**. Selecting another layer, or turning interactive mode off, doesn't wait for the scan: the waiting values are not written, and **Not saved: the scenes were still being scanned** shows at the top-left of the preview until your next click. When no scan is running (the scan failed, the element was not found, or the scenes are not scanned), such a value is refused with the line's text.

A drag, a scrub, or a text edit never waits for the scan. One that would write such a value doesn't begin while the line shows anything but the tabs, and says why with the line's text; one already under way is refused with it when you release or commit. One that began while the scenes were being scanned, writing a value set at the call, and ends after the scan found the element shared, is refused with **The scenes were scanned while editing; edit again**, since you didn't see the options when you began.

## Change timing on the timeline

In interactive mode, the timeline changes when the selected element is drawn and how long a scene lasts, and Celesta writes the result into your source. Nothing is written while you drag: the bands or the chapters, and a label beside the edge you hold, follow the pointer, and the preview keeps showing the frame at the playhead. The value is written once, when you release the mouse. A press that moves less than 3 pixels is a click and jumps as a click on the band or chapter does. Press Escape before releasing to put everything back without writing; starting playback also ends a drag without writing. Values are whole frames, and a drag can go past the edges of the timeline's view.

While you drag, the edge you hold snaps to the playhead, to chapter edges, and to the edges of other bands that stay where they are, when it comes within 6 pixels of one; hold Shift to turn snapping off. An edge never snaps to where it started.

### Drag an element's band

When the selected element sits in a `<Sequence>` or a `<Series.Sequence>` of its own, the filled band at the playhead takes grips at that window's edges: the nearest one around the element counts, imported from `@celesta/react` under any name, such as `Seq` or `C.Sequence`. The pointer turns into a resize arrow over an edge; drag an edge to move it, or the middle to move the whole window:

| Grip | `<Sequence>` writes | `<Series.Sequence>` writes |
| --- | --- | --- |
| Start edge | `from` and `durationInFrames`, so the end stays | `offset` and `durationInFrames` |
| End edge | `durationInFrames` | `durationInFrames` |
| Middle | `from` | `offset` |

- The values are frames of the timeline the window is placed on, such as its scene's: `from={30}` dragged 15 frames to the right becomes `from={45}`.
- A prop you left out starts from what the band shows, `from` and `offset` from 0 and `durationInFrames` from the window's length, and the drag adds it. Dragging the middle of a `<Sequence>` without `durationInFrames` adds it too, at the length shown, since without it the window would keep ending where its parent ends.
- A window keeps at least one frame, and a `<Series.Sequence>` doesn't start before its `<Series>`.
- Dragging the end or the middle of a `<Series.Sequence>` moves the items after it in the same `<Series>`, as `<Series>` places them. Dragging its start edge leaves them where they are.
- Every band of the element drawn through the window moves with the drag, and so do those of the items after it in a `<Series>` when you drag the end edge or the middle.

The label names what changes, such as `from 30 → 45`, or `from 30 → 45 · durationInFrames 60 → 45` for the start edge. Dragging the middle of a `<Sequence>` without `durationInFrames` names the length it adds too, such as `from 30 → 45 · durationInFrames 60`. A `<Sequence>` whose frames are exactly those of the scene it is in, or of a part of it, counts as the scene's: it has no grips, and the element is treated as one without timing of its own (see "Give an element its own timing" below).

Each value is written by the rules of "Move layers on the canvas": a number is replaced, a prop you left out is added, a `const` used once is replaced where it is declared, and a prop of your component, such as `start` in `<Sequence from={start}>`, is written where the component is called. Whether the window is shared depends on the window, not on the selected element. It is shared when it is written inside one of your components and more than one call of that component draws it, such as a component that two scenes draw, or a `Delayed` component that wraps its children in a `<Sequence>` and that each scene calls around elements of its own. A window written once, such as a `<Sequence>` in one scene around a component other scenes draw too, is written where it is, whatever option the element shows. A shared window follows the option chosen above the property sections, as described under "Edit one scene or every scene", or **This scene** when the selected element shows no options, since only its window is shared: **This scene** moves the values the drag writes into new props of the component, such as `sequenceFrom` for the middle, and passes the new values at this scene's call; **Shared value** changes every scene that doesn't set its own. The label then has a second line, **Edits change this scene** or **Edits change the shared value**. As with a drag on the canvas, a grip that option would refuse doesn't drag and shows the reason, such as **Scanning scenes…** while the scenes are scanned, or **This scene sets its own `sequenceFrom`** under **Shared value** when this scene's call passes that prop. The drag writes under the option in effect when you pressed; if that option can't write it by the time you release, nothing is written.

A grip that can't drag shows a not-allowed pointer, and hovering over it shows why. A grip is locked by the reason of a prop it writes, as listed under "Move layers on the canvas": `from={cut - 9}` locks the start edge and the middle with **Written as an expression: cut - 9**, and the end edge still drags. Every grip is locked by reasons such as these:

| Reason | Meaning |
| --- | --- |
| Timing inside a loop or freeze frame can't be dragged | The window nearest the element is inside a `<Loop>` or a `<FreezeFrame>` |
| Not a `<Sequence>` or `<Series.Sequence>` of @celesta/react | The window comes from another wrapper, or from a `Sequence` your file doesn't import from `@celesta/react` |
| Couldn't find this element in the source | The file no longer has the window where the preview found it, such as while a save is still reloading |
| The source is still being read | Celesta is reading the element's source, such as right after you select it |
| Select the layer again | What Celesta read for the layer is no longer current, such as right after one of its own writes |
| This file is outside the project | The window is written in a file outside the entry file's folder, or inside `node_modules` |
| Not saved: the code failed to reload | The composition failed to reload after one of Celesta's writes |
| Not saved: the preview stopped unexpectedly | The preview stopped; restart Celesta |

When a drag is released and nothing is written, a note beside the grip says why, starting with **Not written:**, until your next click: the option you dragged under can't write it any more, such as **Not written: The scenes were scanned while editing; edit again**, the source was reloaded during the drag, or the write failed, such as **Not written: The source changed while editing**.

### Drag a scene's end

On the **Chapters** row, the end of a chapter drags when the chapter comes from a `defineScenes` entry Celesta can write. Dragging it changes the entry's `durationInFrames`, and the scenes after it in the same list move, as `defineScenes` places them. Over an end that drags, the pointer turns into a resize arrow and the end is highlighted. The label shows the change, such as `durationInFrames 90 → 100`.

```ts
const SCENES = defineScenes([
  { id: 'intro', durationInFrames: 90, Scene: Intro },
  { id: 'main', durationInFrames: 120, Scene: Main },
]);
```

After every reload, Celesta reads the `.ts` and `.tsx` files in the entry file's folder and below (skipping `node_modules`, `dist`, `.tmp`, and `.git`) for calls of `defineScenes([...])` imported from `@celesta/react` (also under another name, or as `C.defineScenes` with `import * as C`) whose list is written out: an array of `{ … }` entries whose `id` is a string. A list with anything else in it, such as `...CHAPTERS.map(…)`, is skipped. The chapter's id must belong to one entry only, and that entry's `durationInFrames` must be a number, or a `const` of the same file that nothing else reads and the file doesn't export.

The composition's length follows the drag when the entry file writes it as the list's length, such as `<Composition durationInFrames={SCENES.durationInFrames}>`, where `SCENES` is the list's `const` in the same file, or is imported by name (also under another name) from a path without an extension, such as `import { SCENES } from './timeline'` for `timeline.ts`, `timeline.tsx`, or a `timeline` folder's `index.ts`. Then lengthening a scene lengthens the composition. Otherwise, such as with `durationInFrames={300}`, the composition keeps its length: the drag stops before a scene after the dragged one would start at or after the composition's end, and before the dragged end passes it, and **The composition is 300 frames long** shows beside the end.

- A scene can shrink to 0 frames, which keeps it in the list but never plays it, and no scene after it starts before the list's first scene.
- A chapter cut short by the composition's end is written as the length you see plus the drag; dragging it back to exactly its entry's length writes nothing.
- Only the scenes after the dragged one in its list move. Chapters from other lists, and chapters you list yourself, such as with `listChapters()`, stay where they are.
- The end snaps to the playhead and to the edges of the chapters that don't move.
- A scene list is not a prop of a component, so **Edits change** doesn't apply.

An end that can't drag shows a not-allowed pointer, and hovering over it shows why:

| Reason | Meaning |
| --- | --- |
| Reading source… | Celesta is still reading the files for the current reload |
| Can't find scene `<id>` in the source | No written-out list has an entry with this id, or more than one does, such as for a scene of a list with a spread |
| Not a `defineScenes` entry | The chapter is a part inside another chapter, such as one listed with `listChapters()`, and no list has an entry with its id |
| Written as an expression: `<expression>` | The length is computed, such as `S.field - S.words`; the next lines of the tooltip list each name it reads, with its value, such as `BAR = 30`, or with why it can't be traced |
| Exported, so other files may read it | The length is a `const` the file exports, so changing it could change other files |
| `<name>` is used in `<count>` places | The `const` is read elsewhere too |
| Scene `<id>` has no durationInFrames | The entry leaves out its length |

If the entry changed or got locked by the time you release, or the write fails, nothing is written and the label says why, such as **Not written: The source changed while editing**, until your next click.

### Give an element its own timing

An element without a window of its own, such as a background drawn for its whole scene, has no grips. Instead, a **Set timing** button sits at the start of its filled band at the playhead. Clicking it pauses playback and wraps the element in a `<Sequence>` that starts at the playhead and runs to the end of the scene:

```tsx
// before: the playhead is 45 frames into a 120-frame scene
<Rect width={1280} height={720} fill="#14161C" />

// after
<Sequence from={45} durationInFrames={75}>
  <Rect width={1280} height={720} fill="#14161C" />
</Sequence>
```

- `from` counts from the start of the scene the element is drawn in. An element placed on the composition's own timeline, outside any `<Scenes>` or `<Sequence>`, gets the playhead's frame as `from` and runs to the end of the scene at the playhead, or to the composition's end outside every chapter.
- The `<Sequence>` is indented like the element and the element one level deeper, with the tabs or spaces your file uses; nothing else is reformatted. An element that shares its line with other code, such as `<Group><Rect /></Group>`, is wrapped on that line.
- `Sequence` is added to the file's import from `@celesta/react`, in order when its names are sorted, or on a new import line after the last import. A file that imports it under another name gets that name, such as `<Seq>` or `<C.Sequence>`.
- It is one change: one undo removes the `<Sequence>` and the import together.

The button's tooltip says **Animations inside start at the new start**. Inside a `<Sequence>`, `useCurrentFrame()` counts from the `<Sequence>`'s start, so a component inside the new `<Sequence>` that reads the frame, such as a title fading in with `interpolate(frame, [0, 20], [0, 1])`, now plays its animation from the playhead instead of the scene's start. That is what `<Sequence>` does, and Celesta leaves it so. A value computed outside the new `<Sequence>`, such as an `opacity` the scene's component computes from its own `frame`, keeps its timing; the element is just drawn on fewer frames.

For an element written inside one of your components that several scenes draw, **Set timing** writes the component, so every scene changes. It follows the option chosen above the property sections: it works under **Shared value**, and the `from` then counts from each scene's start. The `durationInFrames` is what is left of the current scene after the playhead and is written for every scene, so in a longer scene the element ends before the scene does. The button is disabled with the reason when it can't write:

| Reason | Meaning |
| --- | --- |
| Wrap the element in a parent first | The element is not a child of another element: it is the root of what a component returns, a prop's value, or written inside `{…}`, such as `{show && <Rect />}` or `.map()` |
| `Sequence` here is not Celesta's | The file uses the name `Sequence` for something else: its own declaration, a type-only import, or an import from another module |
| No frame left after the playhead | The playhead is at the end of the scene |
| Can't be set per scene yet | The element is shared and **This scene** is chosen; choose **Shared value** to give every scene the timing |
| Scanning scenes… | The scan hasn't told yet whether the element is shared; when it can't tell, one of the other texts under "While the scenes are scanned" shows instead |
| This file is outside the project | The element's file is outside the entry file's folder, or inside `node_modules` |

While Celesta reads the element's source, such as right after you select it or move the playhead, the button is hidden. If the write fails, such as when the file changed meanwhile, a note beside the band says why, starting with **Not written:**, until your next click.

## Undo

Undo (`⌘ Z`, `Ctrl Z`) and redo (`⌘ Shift Z`, `Ctrl Shift Z`) step through the changes Celesta wrote, whether from the canvas, the property rows, or the props file's **Properties** section, in one history that holds up to 100 changes and is cleared when you open another file. Each committed row, switch, menu choice, picked color, chosen file, scrub, removed prop, text edited on the canvas, dragged band or scene end, and **Set timing** is one change. A change that wrote two files, such as a component's call site and the component, is undone and redone as one step. If a file the change wrote was edited elsewhere since, Celesta refuses to undo it rather than overwrite your edit, and neither file changes.
