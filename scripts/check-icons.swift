// Verify the shipped artwork, including the iconutil packaging round-trip.
import AppKit
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let assets = root.appendingPathComponent("assets/icons")
let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: temporary) }

func check(_ url: URL, pixels: Int, inset: Int) throws {
  let data = try Data(contentsOf: url)
  guard let image = NSBitmapImageRep(data: data) else { fatalError("Cannot decode \(url.path)") }
  precondition(image.pixelsWide == pixels && image.pixelsHigh == pixels, "Wrong size: \(url.path)")
  precondition(image.hasAlpha, "Missing transparency: \(url.path)")
  var minX = pixels, minY = pixels, maxX = -1, maxY = -1
  for y in 0..<pixels {
    for x in 0..<pixels {
      // Ignore negligible antialiasing outside the tile's mathematical bounds.
      if image.colorAt(x: x, y: y)!.alphaComponent > 0.01 {
        minX = min(minX, x)
        minY = min(minY, y)
        maxX = max(maxX, x)
        maxY = max(maxY, y)
      }
    }
  }
  precondition(
    minX == inset && minY == inset && maxX == pixels - inset - 1 && maxY == pixels - inset - 1,
    "Wrong tile bounds in \(url.lastPathComponent): \(minX),\(minY)–\(maxX),\(maxY)")
  // The middle of the paw pad must remain cream even at 16px and in the
  // worktree variant. This also rejects stale ram artwork with valid margins.
  let paw = image.colorAt(x: pixels / 2, y: pixels * 5 / 8)!.usingColorSpace(.sRGB)!
  precondition(paw.redComponent > 0.95 && paw.greenComponent > 0.90 && paw.blueComponent > 0.85,
               "Missing cream paw: \(url.path)")
}

for name in ["sheprd-icon.png", "sheprd-worktree-1024.png"] {
  try check(assets.appendingPathComponent(name), pixels: 1024, inset: 100)
}

// Expected values transcribed from Apple's Sequoia production template,
// independent of the generator's sizing calculations.
let representations = [
  (16, 1, 1), (16, 2, 2), (32, 1, 2), (32, 2, 6),
  (128, 1, 12), (128, 2, 25), (256, 1, 25), (256, 2, 50),
  (512, 1, 50), (512, 2, 100),
]
for name in ["Sheprd", "Sheprd-worktree"] {
  let iconset = temporary.appendingPathComponent("\(name).iconset")
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
  process.arguments = ["-c", "iconset", assets.appendingPathComponent("\(name).icns").path, "-o", iconset.path]
  try process.run()
  process.waitUntilExit()
  precondition(process.terminationStatus == 0, "iconutil failed")
  for (size, scale, inset) in representations {
    let suffix = scale == 2 ? "@2x" : ""
    try check(iconset.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"), pixels: size * scale, inset: inset)
  }
  print("PASS: \(name).icns — all 10 standard/Retina representations")
}
print("PASS: embedded PNG dimensions and transparent margins")

for target in ["SheprdRemote", "SheprdRemoteWatch"] {
  let url = root.appendingPathComponent("ios/\(target)/Assets.xcassets/AppIcon.appiconset/AppIcon.png")
  let image = NSBitmapImageRep(data: try Data(contentsOf: url))!
  precondition(image.pixelsWide == 1024 && image.pixelsHigh == 1024)
  for y in 0..<1024 {
    for x in 0..<1024 {
      precondition(image.colorAt(x: x, y: y)!.alphaComponent == 1, "Mobile icons must be opaque")
    }
  }
}
print("PASS: iPhone/iPad and Watch catalogs are opaque 1024px icons")

for name in ["Sheprd", "Sheprd-worktree"] {
  let process = Process()
  let output = Pipe()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
  process.arguments = ["assetutil", "--info", assets.appendingPathComponent("\(name).car").path]
  process.standardOutput = output
  try process.run()
  let data = output.fileHandleForReading.readDataToEndOfFile()
  process.waitUntilExit()
  precondition(process.terminationStatus == 0, "assetutil failed")
  let entries = try JSONSerialization.jsonObject(with: data) as! [[String: Any]]
  precondition(entries.contains { $0["Name"] as? String == "Sheprd_Assets/paw" && $0["AssetType"] as? String == "Vector" },
               "Catalog must contain the paw vector")
  precondition(!entries.contains { ($0["Name"] as? String ?? "").contains("Herdr") }, "Stale catalog branding")
  for appearance in ["NSAppearanceNameAqua", "NSAppearanceNameDarkAqua"] {
    precondition(entries.contains { $0["Name"] as? String == "Sheprd" && $0["Appearance"] as? String == appearance },
                 "Missing \(appearance) catalog variant")
  }
}
print("PASS: both compiled catalogs contain Sheprd paw vectors and light/dark variants")
