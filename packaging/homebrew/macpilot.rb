# Homebrew cask for MacPilot, published to the tap github.com/p17m0/homebrew-tap (Casks/macpilot.rb):
#   brew install --cask p17m0/tap/macpilot
# The release workflow fills in `version` and `sha256` (scripts/cask.sh) and pushes it to the tap.
cask "macpilot" do
  version "0.8.2"
  sha256 "c0714f96c377e84964087f65dd2ade012e68ee9a23080cf3e83aeaf433978243"

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
