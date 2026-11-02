import UIKit

/// A picture as the daemon takes it: JPEG under its 3.5 MB limit, at most
/// 1568 points on its long side. Agents shrink larger pictures to about
/// that before reading them, and the bytes travel twice on a phone's link:
/// up as the upload, and back down as the chat shows the prompt's picture.
struct PreparedImage {
    static let maxBytes = 3_400_000
    static let maxSide: CGFloat = 1568

    let jpeg: Data
    let thumbnail: UIImage

    init?(_ image: UIImage) {
        let scaled = Self.scaled(image, to: Self.maxSide)
        var quality: CGFloat = 0.75
        var data = scaled.jpegData(compressionQuality: quality)
        while let bytes = data, bytes.count > Self.maxBytes, quality > 0.3 {
            quality -= 0.15
            data = scaled.jpegData(compressionQuality: quality)
        }
        guard let data, data.count <= Self.maxBytes else { return nil }
        jpeg = data
        thumbnail = Self.scaled(image, to: 160)
    }

    /// A picture prepared before, from a saved draft.
    init?(jpeg: Data) {
        guard let image = UIImage(data: jpeg) else { return nil }
        self.jpeg = jpeg
        thumbnail = Self.scaled(image, to: 160)
    }

    static func scaled(_ image: UIImage, to side: CGFloat) -> UIImage {
        let longest = max(image.size.width, image.size.height)
        guard longest > side else { return image }
        let factor = side / longest
        let size = CGSize(width: image.size.width * factor, height: image.size.height * factor)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }
}
