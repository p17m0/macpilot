# Contributing

Thanks for helping! Bug reports, translations and new cleanup rules are all welcome.

## Build and check

```bash
cargo run --bin macpilot-gui
cargo test
cargo fmt
cargo clippy --all-targets -- -D warnings
```

CI runs the same checks on every pull request.

## Safety first

MacPilot runs on people's real machines. Any change that can remove files must:

1. go through `trash::move_to_trash` (never `remove_file` / `remove_dir_all`);
2. respect `disk::deletion_safety` — add a test in `src/disk.rs` for any new rule;
3. never pre-select things that may hold user documents.

## Adding a translation

1. Add the language to `Lang` in `src/i18n.rs`.
2. Add a column to every row of `src/i18n/strings.rs`.
3. `cargo test` checks that every string is translated and that `{0}` placeholders match.

User-visible text in code is always English wrapped in `tr("…")` or `trf("… {0}", &[&value])`.

## Adding a cleanup place or a build-folder kind

- Known junk locations: `src/clean.rs` (`targets()`).
- Build/dependency folders: `artifact_kind` in `src/devjunk.rs` — detect them by a marker file next to them (like `package.json` for `node_modules`).
- Process descriptions: `known()` in `src/procs.rs`.

## Screenshots for the README

Build with `--features dev-tools` and run with `MACPILOT_SHOT=shot.png MACPILOT_PAGE=disk:map MACPILOT_LANG=en` — the app captures itself and exits. Please use a test account so no personal files appear.

## Screenshots

The images in `docs/screenshots/` are taken by the app itself (feature `dev-tools`):

```sh
cargo build --release --features dev-tools --bin macpilot-gui
MACPILOT_THEME=dark MACPILOT_LANG=en MACPILOT_PAGE=disk:map MACPILOT_SHOT=/tmp/disk_map_dark.png ./target/release/macpilot-gui
sips -Z 1600 /tmp/disk_map_dark.png --out docs/screenshots/disk_map_dark.png
```

`MACPILOT_PAGE` takes `overview`, `procs`, `disk`, `disk:map`, `clean`, `clean:dev`, `apps`, `startup` or `settings`, and `MACPILOT_THEME` takes `light` or `dark`. The README shows `<name>_light.png` or `<name>_dark.png`, whichever matches the reader's theme. Keep the window in front while it captures: macOS does not draw windows that are covered.

## Releases

See [docs/RELEASING.md](docs/RELEASING.md): tagging, signing with a Developer ID, notarization and the Homebrew cask.
