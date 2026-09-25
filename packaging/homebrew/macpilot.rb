# Homebrew cask for MacPilot. Put it in a tap repository named `homebrew-tap`
# (github.com/OWNER/homebrew-tap, file Casks/macpilot.rb); then users run:
#   brew install --cask OWNER/tap/macpilot
# On every release update `version` and `sha256` (the zip's line in SHA256SUMS.txt).
cask "macpilot" do
  version "0.3.0"
  sha256 "REPLACE_WITH_SHA256_OF_THE_ZIP"

  url "https://github.com/OWNER/macpilot/releases/download/v#{version}/MacPilot-v#{version}-macos-universal.zip"
  name "MacPilot"
  desc "Lightweight cleaner and system monitor"
  homepage "https://github.com/OWNER/macpilot"

  depends_on macos: ">= :monterey"

  app "MacPilot.app"
  # The terminal app ships inside the bundle.
  binary "#{appdir}/MacPilot.app/Contents/Helpers/macpilot"

  zap trash: [
    "~/Library/Application Support/MacPilot",
    "~/Library/Caches/MacPilot",
    "~/Library/LaunchAgents/local.macpilot.plist",
  ]
end
