# Releasing MacPilot

A release is built by GitHub Actions when you push a tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

The workflow (`.github/workflows/release.yml`) runs `scripts/release.sh`. It builds universal binaries and attaches these files to the GitHub release:

| File | For |
|---|---|
| `MacPilot-v0.2.0.dmg` | people downloading by hand: open it and drag the app to Applications |
| `MacPilot-v0.2.0-macos-universal.zip` | the Homebrew cask |
| `macpilot-cli-v0.2.0-macos-universal.tar.gz` | the terminal app on its own |
| `SHA256SUMS.txt` | checksums |

Before tagging, update `version` in `Cargo.toml` and add a section to `CHANGELOG.md`.

## Signing and notarization

Without extra setup, the release is **ad-hoc signed**. It works, but:

- macOS says the app is from an unidentified developer, so users must right-click → Open once;
- permissions (Full Disk Access, controlling Finder) are lost after every update, because each build looks like a different app to macOS.

With a **Developer ID** the app is signed, notarized by Apple and opens normally, and permissions survive updates.

### 1. Join the Apple Developer Program

<https://developer.apple.com/programs/> costs $99 a year. Your Team ID is shown under **Membership details**.

### 2. Create a Developer ID Application certificate

1. On your Mac, open **Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority…**. Enter your email, choose **Saved to disk**, and save the `.certSigningRequest` file.
2. At <https://developer.apple.com/account/resources/certificates>, click **+**, choose **Developer ID Application**, and upload the request.
3. Download the certificate and double-click it to add it to your keychain.
4. In Keychain Access, find **Developer ID Application: Your Name (TEAMID)**. Right-click it → **Export…** and save it as a `.p12` file with a password.

### 3. Create credentials for notarization

**Option A: App Store Connect API key (recommended; works without two-factor prompts).**
At <https://appstoreconnect.apple.com/access/integrations/api>, create a key with the **Developer** role. Download the `.p8` file (you can only download it once) and note its **Key ID** and the **Issuer ID**.

**Option B: Apple ID.**
At <https://account.apple.com>, go to **Sign-In and Security → App-Specific Passwords** and create a password.

### 4. Add the GitHub secrets

Go to your repository → **Settings → Secrets and variables → Actions → New repository secret**:

| Secret | Value |
|---|---|
| `MACOS_CERTIFICATE` | `base64 -i DeveloperID.p12 \| pbcopy`, then paste |
| `MACOS_CERTIFICATE_PASSWORD` | the password of the `.p12` file |
| `APPLE_API_KEY` | option A: contents of the `.p8` file |
| `APPLE_API_KEY_ID` | option A: Key ID |
| `APPLE_API_ISSUER` | option A: Issuer ID |
| `APPLE_ID` | option B: your Apple ID email |
| `APPLE_TEAM_ID` | option B: your Team ID |
| `APPLE_APP_PASSWORD` | option B: the app-specific password |

The next tag builds a signed, notarized release.

### Releasing from your Mac instead

```bash
xcrun notarytool store-credentials macpilot --apple-id you@example.com --team-id TEAMID
CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" NOTARY_PROFILE=macpilot scripts/release.sh v0.2.0
```

Check the result:

```bash
spctl --assess --type execute --verbose dist/MacPilot.app
xcrun stapler validate dist/MacPilot-v0.2.0.dmg
```

## Homebrew

`packaging/homebrew/macpilot.rb` is a ready-made cask:

1. Create a public repository named `homebrew-tap`.
2. Copy the file to `Casks/macpilot.rb` in it.
3. Replace `OWNER` with your GitHub user name.
4. On each release, set `version` and `sha256`. The value for `sha256` is the zip's line in `SHA256SUMS.txt`.

Users then install MacPilot with `brew install --cask OWNER/tap/macpilot`. This installs both the app and the `macpilot` command.
