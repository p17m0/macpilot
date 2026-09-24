# Changelog

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

