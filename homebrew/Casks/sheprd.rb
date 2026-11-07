cask "sheprd" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/sheprd-labs/sheprd/releases/download/v#{version}/Sheprd-#{version}-universal-apple-darwin.dmg"
  name "Sheprd"
  desc "Sheprd native workspace for coding agents and terminals"
  homepage "https://sheprd.org"

  depends_on macos: :sonoma

  app "Sheprd.app"
  binary "#{appdir}/Sheprd.app/Contents/MacOS/sheprd"
end
