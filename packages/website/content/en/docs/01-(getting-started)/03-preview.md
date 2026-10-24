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
- **Shared** appears in the warning color when the same element is also drawn in other scenes, such as **also in Intro, Outro (3 instances)**. With more than three other scenes, it counts them instead, such as **also in 11 other scenes (71 instances)**. Each place your code renders the element, such as each `<Title>` you wrote, is one instance. Hover over the row to list the `file:line:column` each instance is rendered from, after the scene names when the row only counts them.
- **Can't move** lists the transform props that are locked and why, one reason per row, such as **X, Y — Written as an expression: W / 2**. The reasons are explained in the next section.

Above the property sections, one line says what committing a change would do:

| Line | Meaning |
| --- | --- |
| Changes this scene only | The element is drawn in one place, and in no other scene |
| Changes `<count>` places in this scene | The element is drawn in more than one place in the current scene, such as by two `<Title>`s or in `.map()`, and in no other scene. The moments `<MotionBlur>` and `<Trail>` draw count as one place |
| Also changes `<scenes>` | The element is also drawn in the scenes named, shown in the warning color. With the playhead between chapters, every scene the element is in is named. More than three scenes are counted instead, such as **Also changes 11 other scenes**; hover over the line to list them |
| Scanning scenes… | The scan has not finished yet |
| Couldn't scan scenes: `<error>` | The composition failed on some frame during the scan; the rest of the preview works as before |
| Not found in the scene scan | The scan did not see this element, such as when it has no source position |

When `x`, `y`, `rotation`, and `scale` (or `scaleX` and `scaleY`) are all props of your component, set at the call (see the next section), a change writes only that call, so only the instances that call renders count; a locked prop does not matter. With `const PriceTag = ({ x = 0, y = 0, rotation = 0, scale = 1 }) => <Rect x={x} y={y} rotation={rotation} scale={scale} />` used in Intro and Price, selecting it in Price, where it is called as `<PriceTag x={40} />`, reads **Changes this scene only**: moving it writes that call and leaves the one in Intro alone. If any of them is written on the element itself, or left out of it as with `({ x }) => <Rect x={x} />`, a change can write the element, which every instance draws, and every instance counts. The **Shared** row and the timeline bands count the same instances as the line.

On the timeline, the **Composition** row marks where the element is drawn: a filled band for the selected layer in the current scene, and an outlined band for each other place an edit of it also changes. Hover over a band to see its scene (or its timecodes in a composition without chapters), its frames, and where that instance is rendered from; click a band to jump to its first frame. Bands of one instance less than a pixel apart at the current zoom are drawn as one, whose tooltip says how many ranges it covers. In the **Chapters** row, the chapters the selected layer appears in are underlined. Until the scan finishes, a single filled band shows the frames the layer's sequences let it be drawn in.

## Move layers on the canvas

In interactive mode, a selected layer can be moved, rotated, and scaled, and the result is written into your source. Drag inside the outline to move it, drag a square handle at a corner or an edge to scale it about its anchor, and drag the handle above the top edge to rotate it (hold Shift to snap to 15°). You can also type a value into the **Transform** rows below the header of the **Properties** tab and press Enter or leave the field; the next section describes every row of the tab. One drag or one committed field is one change, even when it writes two files: Celesta replaces only the number it changes and leaves the rest of your formatting and comments as they are, and the preview keeps the selection when it reloads.

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
| Rendered `<count>` times from this element | An element in `.map()`; editing it would move every copy |
| Can't tell which frame this animation reads | `frame` could hold more than one frame (each `<Sequence>` or `<FreezeFrame>` around the element gives another), and those frames give different keyframes with the same value |
| This file is outside the project | The value is written in a file outside the entry file's folder, or inside `node_modules`: the element's file, or the call site's for a value set there |
| Couldn't find this element in the source | The file no longer has the element where the preview found it, such as while a save is still reloading |
| The source has syntax errors | Fix the file first; every field is locked until it parses |

If a file the drag would write changes while you drag, Celesta discards the drag and says so instead of writing over your edit. Dragging on the canvas edits only `x`, `y`, `rotation`, and `scale` (or `scaleX` and `scaleY`, when written); size props such as `width` and the anchor are edited in the **Properties** tab, as the next section describes, and skew is not edited.

A layer inside a `<Group>` or `<Stack>` moves with its parent. A child of a `<Stack>` stays editable: its `x` and `y` are offsets from the place the `<Stack>` gives it. Under **Source chain**, each row of **Called from** that is a Celesta element drawing its own layer, such as a `<Group>`, has a **Select** button that selects that layer; a group is outlined as the box around its children, and dragging anywhere inside it moves the whole group. A `<Stack>` or `<SafeArea>` row has no **Select** button: they are components, and the groups they create are not written in your source.

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

To remove a prop your source writes, so its default applies again, empty its field and press Enter (or leave the field), or hover the row and click the **×** at its end; switches and menus have the **×** too. The attribute goes from the element, a property from its `style={{ … }}`, or, in a row with **Set at**, the attribute from the call, so your component's default applies. The row then shows, dimmed, the value drawn without it. Props an element can't do without, such as a `<Rect>`'s **Width** and **Height**, an `<Image>`'s **Source**, or the **Text** of a `<Text>`, have no **×**, and emptying their field puts the value back. The same goes for props your source leaves out, locked rows, rows animated with keyframes, and input rows.

While Celesta reloads after writing a value, the rows stay editable: the field you typed in keeps the keyboard focus, you can go on typing, and a value you commit meanwhile is written once the reload finishes; until then, its row says **Saving…**. Leaving a field commits it, also when you leave it by selecting another layer, clicking an empty part of the preview, moving the playhead, or starting playback: what you typed is written as if you had pressed Enter, and text the row can't take is dropped. When a value is still waiting for the reload, the new selection shows once that value is written. Turning interactive mode off waits the same way: until the value is written, the **Select layers** button shows a spinner and the layer stays selected, and turning the mode on again meanwhile keeps it on with the selection. If the code fails to reload, the waiting values are not written, and their rows say **Not saved: the code failed to reload**. If the preview stops unexpectedly, they say **Not saved: the preview stopped unexpectedly**, and no value is written from the rows until you restart Celesta.

To scrub a number, drag its label left or right. Each pixel changes the value by the prop's step: 0.01 for **Scale**, **Scale X**, **Scale Y**, **Anchor X**, **Anchor Y**, and a video's **Playback rate** and **Volume**, 100 for **Font weight** (a font's faces come in weights 100 apart), and for a component prop, the `step` its `registerComponent()` schema declares. A prop without a step changes by 0.01 when its range ends at 1 or below, such as **Opacity**, and by 1 otherwise. Holding Shift makes each step ten times larger. A value stops at the prop's range. A prop your source leaves out starts from the dimmed value its row shows. The preview follows while you drag, including the other layers drawn from the same value, and the value is written once, when you release the mouse. Press Escape before releasing to put everything back without writing; releasing without moving, or with the value back where it started, writes nothing either. Text you typed in the row you scrub gives way to the scrubbed value. Text you typed but haven't committed in another row stays in its field, which keeps the keyboard focus; otherwise pressing a label moves the focus out of the fields, so `⌘ Z` (`Ctrl Z`) right after a scrub undoes it. Rows that are locked or animated with keyframes don't scrub, neither do input rows, and labels don't scrub during playback. Dragging a layer on the canvas and scrubbing a label never run together: starting one ends the other without writing.

A value computed from others, such as `width={textWidth + padding * 2}`, stays locked, and the names it reads are listed below its row as indented input rows, such as **textWidth** and **padding**. Each input is traced by the same rules, so a `const` used once, or a prop of your component set at the call (with its own **Set at** line), can be edited there; the others show their lock reason. Inputs are one level deep: an input that is itself an expression is not expanded. A value computed at your component's call, such as `<PriceTag x={left + 8} />`, lists no inputs.

The **Font family**, **Font size**, **Font weight**, and **Color** rows of a `<Text>` follow its `style`, since `style` wins over those props. When `style={{ … }}` sets the value (`fill` for **Color**), the row edits it there. Otherwise the row edits the prop itself, adding `fontSize={48}` beside a `style` that doesn't set it. When `style` is not written as an object, such as `style={mono(20)}`, or a spread in it or on the element could set the value, the row is locked with **May be set by `style`**.

When the element is written inside one of your components and Celesta finds the call that renders it, a **Component** section follows, such as **Component · PriceTag**, with **Set at `<file>`:`<line>`** for that call. It lists the props written at the call in the order they are written, so you can change `label="Sale"` on `<PriceTag>` while a `<Rect>` inside it is selected. A prop a row above already shows with **Set at** is not repeated, and `key`, `ref`, spreads, and children are left out. A number gets a number field, a string a text field, and `true` or `false` a switch. When the component is registered with `registerComponent()`, its schema gives the control and the range instead, a field's `label` names its row (a prop the schema leaves out shows its name as written), a number field with `integer: true` takes whole numbers only, and a value of another kind than the schema's is locked. A value that is not written plainly is locked with its reason.

The impact line and **Can't move** look at the transform props only. Editing another prop written on the element, such as **Fill**, changes every instance even when the line counts only one call's instances; a value set at the call, in a row with **Set at** or in the **Component** section, changes only that call.

## Undo

Undo (`⌘ Z`, `Ctrl Z`) and redo (`⌘ Shift Z`, `Ctrl Shift Z`) step through the changes Celesta wrote, whether from the canvas, the property rows, or the props file's **Properties** section, in one history that holds up to 100 changes and is cleared when you open another file. Each committed row, switch, menu choice, picked color, chosen file, scrub, and removed prop is one change. A change that wrote two files, such as a component's call site and the component, is undone and redone as one step. If a file the change wrote was edited elsewhere since, Celesta refuses to undo it rather than overwrite your edit, and neither file changes.
