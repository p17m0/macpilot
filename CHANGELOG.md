# Changelog

## 0.3.0

- **Signed and notarized releases**: `scripts/release.sh` and the release workflow sign with a Developer ID and notarize when the secrets are set (see `docs/RELEASING.md`). Releases now include a DMG, and there is a Homebrew cask. The `macpilot` command ships inside the app.
- **Instant start**: the last home-folder scan is cached (~7 MB, loads in ~0.1 s) and refreshed in the background.
- **Menu bar item** with CPU and memory, plus disk space and the busiest app in its menu. Closing the window keeps MacPilot running there.
- **Launch at login** is now a regular macOS login item (`SMAppService`); the old LaunchAgent is migrated automatically.
- **Update check**: once a day, against GitHub releases, and it can be turned off.
- **Drag and drop**: drop an app on the window to uninstall it, or a file or folder to find it on the disk.
- **VoiceOver** support (AccessKit).
- **Safety**: `deletion_safety` now also blocks paths with `..` and case variants of protected folders (`~/library/keychains`), and has many more tests.
- Disk header: the used space moved under the bar, so long translations no longer overlap the buttons.
- README screenshots in light and dark.

## 0.2.0

- New app icon, in the app and in the docs.
- Wording pass in all five languages: plain explanations, localized numbers, sizes and plurals.
- Clearer recommendations: memory pressure mentions swap only when it is used, and high CPU is explained (“100% means one fully busy core”).
- MacPilot's own process gets its own label instead of “critical”.
- UI: the disk header and list columns fit, the side panel says “Home folder” instead of `~`, and empty cleanup places are hidden.
- README with screenshots.

## 0.1.0 — first public release

- **Window app** with a sidebar: Overview, Processes, Disk, Cleanup, Apps, Startup, Settings.
- **Overview** with recommendations and one-click actions.
- **Apps**: uninstall with leftovers; find leftovers of removed apps.
- **Startup items**: launch agents/daemons, reversible disable, adware and broken-item detection.
- **Developer junk**: build and dependency folders of all projects, ranked by project inactivity.
- **Duplicates** finder (content-based, opt-in removal, git-project copies flagged).
- **Disk**: *Modified* / *Opened* columns and “Not used” view with safe exclusions (media, app data, tool internals).
- **Open ports** per process.
- **Languages**: English, French, Spanish, German, Russian.
- **Settings**: language, theme, open at login, thresholds.
- Own Trash implementation (Finder, with fallback) — fewer dependencies; scans on separate low-priority pools with timeouts for privacy-protected folders.
- Command-line reports: `stale`, `junk`, `apps`, `leftovers`, `startup`, `dupes`.

