# Changelog

## 0.5.0

- **Classic Macintosh style** (Settings → Style), light and dark: black and white (white on black in the dark theme), 1-pixel lines, square corners, hard drop shadows, striped page titles and a pixel font (Pixelify Sans, SIL OFL). The standard look stays the default.
- The logo in the window follows the style: in color normally, black and white in Classic.
- Page content is centered and at most 1180 px wide on large windows.
- **Disk → Summary**: the whole disk in one bar — your home folder, apps and other folders, macOS, swap, free space, and how much MacPilot cannot see (usually other apps' data such as Docker or virtual machine images, visible with Full Disk Access). Local Time Machine snapshots are listed.
- **What changed**: MacPilot keeps a small daily summary of the home folder (~30 KB a day, 35 days) and shows which folders grew or shrank over 1, 7 or 30 days. The Overview warns when your files grew by 5 GB or more.
- **Notifications** (can be turned off): an app stuck spawning hundreds of processes, a nearly full disk, a hot battery, an app draining the battery or keeping the Mac awake on battery power. A click opens the right page.
- **Less memory**: the folder map of a scan is a compact tree (18 MB instead of ~50 MB for 250 000 folders), and measuring cleanup targets, apps and leftovers no longer builds throwaway folder maps. The scan cache format is v2 (rebuilt once).
- The Overview says how much of the disk is hidden without Full Disk Access.

## 0.4.0

- **Battery page**: charge and time left, power drawn right now, health and condition exactly as System Settings reports them, charge cycles and temperature. A 24-hour / 7-day charge chart (MacPilot logs the charge every 5 minutes), average drain per hour, apps using energy now, and what keeps the Mac awake. Advice when the battery needs service, is hot, is near its rated cycles, or is low without Low Power Mode.
- **Energy column** in Processes: watts per process and per app (processor and graphics), from macOS's own energy counters.
- Overview warns about apps draining the battery, apps keeping the Mac awake on battery power, and battery health.
- The battery in the sidebar and the menu bar menu; `macpilot battery` in the terminal.
- On battery MacPilot uses today's scan cache instead of rescanning at launch, and refreshes it once the charger is connected.
- Segmented controls keep their order inside right-aligned rows (and for VoiceOver).

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
- **Permissions are asked once**: `./install.sh` signs with a stable local certificate (`scripts/local-signing.sh`), so macOS keeps Full Disk Access and the Finder permission across rebuilds. Without Full Disk Access, other apps' containers are skipped instead of triggering a dialog on every launch, and a fresh scan starts as soon as access is granted.
- The Dock icon is MacPilot's own again (eframe no longer replaces it with its logo).

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

