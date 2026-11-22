import AppKit

struct ScreenshotComparison {
    let overlay: CGImage
    let changedPixels: Int
    let areas: [CGRect]

    /// Normalize to the same RGBA layout before comparing the captured pixels.
    static func pixels(before: CGImage, after: CGImage) -> Self? {
        let width = before.width, height = before.height
        guard width == after.width, height == after.height,
              width > 0, height > 0, width <= 45_000_000 / height,
              let color = CGColorSpace(name: CGColorSpace.sRGB),
              let left = buffer(for: before, color: color),
              let right = buffer(for: after, color: color),
              let lhs = left.data?.assumingMemoryBound(to: UInt32.self),
              let rhs = right.data?.assumingMemoryBound(to: UInt32.self) else { return nil }

        let tile = 12
        let columns = (width + tile - 1) / tile, rows = (height + tile - 1) / tile
        var tiles = [CGRect](repeating: .null, count: columns * rows)
        var bounds = CGRect.null
        var count = 0
        let colorPixel = UInt32(0xffa418dc).littleEndian
        for y in 0..<height {
            if y % 32 == 0, Task.isCancelled { return nil }
            for x in 0..<width {
                let offset = y * width + x
                if lhs[offset] == rhs[offset] {
                    lhs[offset] = 0
                } else {
                    lhs[offset] = colorPixel
                    let pixel = CGRect(x: x, y: y, width: 1, height: 1)
                    let index = (y / tile) * columns + x / tile
                    tiles[index] = tiles[index].union(pixel)
                    bounds = bounds.union(pixel)
                    count += 1
                }
            }
        }
        guard let overlay = left.makeImage() else { return nil }
        var areas: [CGRect] = []
        for seed in tiles.indices where !tiles[seed].isNull {
            var queue = [seed]
            var index = 0
            var region = tiles[seed]
            tiles[seed] = .null
            while index < queue.count {
                let current = queue[index]
                index += 1
                let x = current % columns, y = current / columns
                for row in max(0, y - 1)...min(rows - 1, y + 1) {
                    for column in max(0, x - 1)...min(columns - 1, x + 1) {
                        let neighbor = row * columns + column
                        guard !tiles[neighbor].isNull else { continue }
                        region = region.union(tiles[neighbor])
                        tiles[neighbor] = .null
                        queue.append(neighbor)
                    }
                }
            }
            areas.append(region)
            if areas.count >= 1_024 { return Self(overlay: overlay, changedPixels: count, areas: [bounds]) }
        }
        return Self(overlay: overlay, changedPixels: count, areas: areas)
    }

    private static func buffer(for image: CGImage, color: CGColorSpace) -> CGContext? {
        let options = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
        guard let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                                      bytesPerRow: image.width * 4, space: color, bitmapInfo: options) else { return nil }
        context.interpolationQuality = .none
        context.setBlendMode(.copy)
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return context
    }
}
