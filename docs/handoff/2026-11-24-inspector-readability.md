# Inspector readability

Use the editor's body text size for Inspector section headings and field
labels. Keep headings distinct with their weight and surface, and field
labels secondary to their values. Long field labels can wrap to two lines;
the existing tooltip still reveals the full text.

The right pane, label and value columns, and action slots keep their widths.
Only longer rows grow, within the existing scroll area. Selection, editing,
and keyboard disclosure use the same controls.

The layout regression test compares a short and a clamped label in a narrow
pane, checking that values and trailing actions remain aligned.
