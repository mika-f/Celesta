# Headless project initialization

`celesta init [DIRECTORY] [--template NAME]` initializes a React project and
`celesta templates` lists the staged starters. The working directory is the
default and the initializer preserves existing files unless a selected
template would collide, in which case it fails without writing. The
desktop editor's `--init` and `--list-templates` are unchanged.

The existing `celesta-cli` binary dispatches these commands directly to
`celesta-react-bridge`'s initializer and template listing; it links no
GPUI and does not spawn Node.js. Both commands locate templates alongside
the React runtime using the shared source/bundle locator.

Validation: `pnpm run build:runtime` for staged templates,
`cargo test --locked -p celesta-cli -p celesta-react-bridge --lib --tests`
for initialization, bundle lookup and plugin compatibility, and run
`celesta templates`, `celesta init` and the old editor flags with
`DISPLAY` and `WAYLAND_DISPLAY` unset.
