# lazyjj

Early keyboard-first TUI prototype for [Jujutsu](https://jj-vcs.github.io/jj/).

## Prototype scope

- Browse revisions, changed files, full diffs, and local/remote bookmarks
- Switch panes with `Tab` or numbered shortcuts
- Add, edit, or clear selected revision description
- Split selected revision with Jujutsu's interactive file/hunk selector
- Create bookmark on selected revision
- Move selected bookmark to selected revision or `@-`
- Push or delete selected local bookmark
- Track selected remote bookmark
- Run `jj undo`
- Preview and confirm every repository-changing command

## Run

Requires Rust and `jj` on `PATH`. Start inside a Jujutsu repository:

```sh
cargo run
```

## Keys

- `1`: focus revisions pane
- `2`: focus bookmarks pane
- `Tab`, `Shift+Tab`: cycle panes
- `j`, `k`, arrows: move selection
- `Enter`, `v`: view selected revision diff
- `e`: edit selected revision description with `jj describe`
- `s`: open lazyjj hunk picker for selected revision
- `n`: create bookmark on selected revision
- `m`: move selected bookmark to selected revision
- `-`: move selected bookmark to `@-`
- `p`: push selected local bookmark
- `t`: track selected remote bookmark
- `d`: delete selected local bookmark
- `u`: undo latest repository operation
- `r`: refresh
- `?`: help
- `q`, `Ctrl+C`: quit

## Current limitations

Prototype runs `jj` synchronously, so slow commands briefly block input. Revision graph uses a flat list. Remote state, search, diff viewer, operation log, configuration, and background refresh remain future work.
