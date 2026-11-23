# Homebrew cask for MacPilot, published to the tap github.com/p17m0/homebrew-tap (Casks/macpilot.rb):
#   brew install --cask p17m0/tap/macpilot
# The release workflow fills in `version` and `sha256` (scripts/cask.sh) and pushes it to the tap.
cask "macpilot" do
  version "0.8.3"
  sha256 "0307df4f26334e538c044cf4b98c84661bdbcce7367974b450655763e829c0af"

  url "https://github.com/p17m0/macpilot/releases/download/v#{version}/MacPilot-v#{version}-macos-universal.zip"
  name "MacPilot"
  desc "Lightweight cleaner and system monitor"
  homepage "https://github.com/p17m0/macpilot"

  depends_on macos: :monterey

  app "MacPilot.app"
  # The terminal app ships inside the bundle.
  binary "#{appdir}/MacPilot.app/Contents/Helpers/macpilot"

  zap trash: [
    "~/Library/Application Support/MacPilot",
    "~/Library/Caches/MacPilot",
    "~/Library/LaunchAgents/local.macpilot.plist",
  ]
end
