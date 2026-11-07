// Render each native icon resolution directly from the canonical vector artwork.
import AppKit
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let assets = root.appendingPathComponent("assets/icons")
let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: temporary) }

// Sequoia's Template - Icon - App.sketch uses pixel-aligned margins that vary
// by resolution. The source tile is 896px across on a 1024px canvas.
let macOSInsets = [16: 1, 32: 2, 64: 6, 128: 12, 256: 25, 512: 50, 1024: 100]

func run(_ executable: String, _ arguments: [String], quiet: Bool = false) throws {
  let process = Process()
  process.executableURL = URL(fileURLWithPath: executable)
  process.arguments = arguments
  if quiet { process.standardOutput = FileHandle.nullDevice }
  try process.run()
  process.waitUntilExit()
  precondition(process.terminationStatus == 0, "\(executable) failed")
}

func artwork(_ name: String, worktree: Bool) throws -> String {
  let source = try String(contentsOf: assets.appendingPathComponent(name), encoding: .utf8)
  // Preserve the cream paw and its antialiasing. A luminance mapping also
  // recolors the foreground and loses the mark's contrast.
  return worktree
    ? source.replacingOccurrences(of: "#f39a5a", with: "#df4b62")
      .replacingOccurrences(of: "#d65f2a", with: "#991f43")
    : source
}

func render(_ source: String, pixels: Int, macOS: Bool) throws -> Data {
  let svg = temporary.appendingPathComponent("render.svg")
  try source.write(to: svg, atomically: true, encoding: .utf8)
  let output = temporary.appendingPathComponent("render.png")
  let width = macOS ? Double(pixels - 2 * macOSInsets[pixels]!) * 1024 / 896 : Double(pixels)
  let offset = (Double(pixels) - width) / 2
  try run("/usr/bin/env", [
    "rsvg-convert", "--width", String(width), "--height", String(width),
    "--page-width", String(pixels), "--page-height", String(pixels),
    "--left", String(offset), "--top", String(offset), "--output", output.path, svg.path,
  ])
  let png = try Data(contentsOf: output)
  guard let image = NSBitmapImageRep(data: png), image.pixelsWide == pixels, image.pixelsHigh == pixels else {
    fatalError("Unexpected SVG render dimensions")
  }
  return png
}

// Icon Composer keeps the original gradient and cream mark as a full-bleed
// vector layer. The system supplies its mask; no extra glass/shadow treatment.
func compileAssetCatalog(name: String, worktree: Bool) throws {
  let document = temporary.appendingPathComponent("\(name)/Sheprd.icon")
  try FileManager.default.createDirectory(at: document.appendingPathComponent("Assets"), withIntermediateDirectories: true)
  try artwork("sheprd-avatar.svg", worktree: worktree)
    .write(to: document.appendingPathComponent("Assets/paw.svg"), atomically: true, encoding: .utf8)
  let manifest: [String: Any] = [
    "fill": ["solid": worktree ? "srgb:0.60000,0.12157,0.26275,1.00000" : "srgb:0.83922,0.37255,0.16471,1.00000"],
    "groups": [[
      "layers": [["glass": false, "image-name": "paw.svg", "name": "Sheprd paw"]],
      "shadow": ["kind": "none", "opacity": 0.0],
      "specular": false,
      "translucency": ["enabled": false, "value": 0.0],
    ]],
    "supported-platforms": ["squares": ["macOS"]],
  ]
  try JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .sortedKeys])
    .write(to: document.appendingPathComponent("icon.json"))
  let output = temporary.appendingPathComponent("\(name)-catalog")
  try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
  try run("/usr/bin/xcrun", [
    "actool", document.path, "--compile", output.path, "--app-icon", "Sheprd",
    "--enable-on-demand-resources", "NO", "--development-region", "en",
    "--target-device", "mac", "--platform", "macosx", "--minimum-deployment-target", "14.2",
    "--output-partial-info-plist", output.appendingPathComponent("partial.plist").path,
  ], quiet: true)
  let destination = assets.appendingPathComponent("\(name).car")
  try? FileManager.default.removeItem(at: destination)
  try FileManager.default.copyItem(at: output.appendingPathComponent("Assets.car"), to: destination)
  print("Generated assets/icons/\(name).car")
}

for worktree in [false, true] {
  let name = worktree ? "Sheprd-worktree" : "Sheprd"
  let source = try artwork("sheprd-icon.svg", worktree: worktree)
  let square = try artwork("sheprd-icon-square.svg", worktree: worktree)
  try render(source, pixels: 1024, macOS: true)
    .write(to: assets.appendingPathComponent(worktree ? "sheprd-worktree-1024.png" : "sheprd-icon.png"))
  try render(square, pixels: 1024, macOS: false)
    .write(to: assets.appendingPathComponent(worktree ? "sheprd-square-worktree-1024.png" : "sheprd-icon-square.png"))
  let iconset = temporary.appendingPathComponent("\(name).iconset")
  try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
  for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
      let suffix = scale == 2 ? "@2x" : ""
      try render(source, pixels: size * scale, macOS: true)
        .write(to: iconset.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
  }
  try run("/usr/bin/iconutil", ["-c", "icns", iconset.path, "-o", assets.appendingPathComponent("\(name).icns").path])
  print("Generated assets/icons/\(name).icns")
  try compileAssetCatalog(name: name, worktree: worktree)
}

for name in ["sheprd-avatar", "sheprd-icon-badge", "sheprd-github-app"] {
  try render(artwork("\(name).svg", worktree: false), pixels: 1024, macOS: false)
    .write(to: assets.appendingPathComponent("\(name).png"))
}
try render(artwork("sheprd-github-app.svg", worktree: false), pixels: 200, macOS: false)
  .write(to: assets.appendingPathComponent("sheprd-github-app-200.png"))

// iOS and watchOS supply their own masks. Their app icons must be opaque and
// full-bleed, without desktop margins or pre-rounded corners.
let mobile = try render(artwork("sheprd-avatar.svg", worktree: false), pixels: 1024, macOS: false)
for target in ["SheprdRemote", "SheprdRemoteWatch"] {
  try mobile.write(to: root.appendingPathComponent("ios/\(target)/Assets.xcassets/AppIcon.appiconset/AppIcon.png"))
  print("Generated \(target) AppIcon.png")
}
