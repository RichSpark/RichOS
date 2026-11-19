import CoreGraphics
import Foundation
import Vision

/// Positions in the stitched image use the same top-left pixel coordinates as a capture crop.
struct ScrollStitcher {
    enum Update: Equatable {
        case initial, unchanged, moved(dx: Int, dy: Int), unmatched, limit
    }

    private struct Offset: Equatable {
        let x: Int
        let y: Int
        static let zero = Offset(x: 0, y: 0)
    }

    private struct Match {
        let offset: Offset
        let matches: Int
        let samples: Int

        var quality: Double { samples == 0 ? 0 : Double(matches) / Double(samples) }
    }

    /// Bands along the edges of the view that stay put between two frames while the content
    /// between them moves: a sticky header, a frozen column.
    private struct Fixed: Equatable {
        var top = 0
        var left = 0
        static let none = Fixed()

        func row(_ y: Int) -> Bool { y < top }
        func covers(_ x: Int, _ y: Int) -> Bool { y < top || x < left }
    }

    private enum Line { case flat, still, moving }

    private struct Patch {
        let rect: CGRect
        let image: CGImage
    }

    static let pixelLimit = 48_000_000
    static let dimensionLimit = 32_000
    private static let patchLimit = 2_000
    private var last: Raster?
    private var position = Offset.zero
    private var patches: [Patch] = []
    private(set) var bounds = CGRect.null

    mutating func append(_ image: CGImage) -> Update {
        guard image.width >= 24, image.height >= 24,
              Self.fits(CGRect(x: 0, y: 0, width: image.width, height: image.height)),
              let next = Raster(image) else { return .limit }
        if let last {
            guard next.width == last.width, next.height == last.height else { return .unmatched }
            if next.pixels == last.pixels { return .unchanged }
            guard let offset = Self.registration(from: last, to: next) else { return .unmatched }
            guard offset != .zero else { return .unchanged }
            let nextPosition = Offset(x: position.x + offset.x, y: position.y + offset.y)
            guard add(next, at: nextPosition) else { return .limit }
            return .moved(dx: offset.x, dy: offset.y)
        }
        return add(next, at: .zero) ? .initial : .limit
    }

    /// Draw each covered pixel once. Areas never visited in a manual pan stay transparent.
    func image(previewLimit: Int? = nil) -> CGImage? {
        guard !bounds.isNull else { return nil }
        let scale = previewLimit.map { min(1, CGFloat($0) / max(bounds.width, bounds.height)) } ?? 1
        let width = max(1, Int((bounds.width * scale).rounded(.up)))
        let height = max(1, Int((bounds.height * scale).rounded(.up)))
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: Raster.colorSpace, bitmapInfo: Raster.bitmapInfo) else { return nil }
        context.interpolationQuality = scale == 1 ? .none : .medium
        context.setBlendMode(.copy)
        for patch in patches {
            context.draw(patch.image, in: CGRect(x: (patch.rect.minX - bounds.minX) * scale,
                                                  y: (bounds.maxY - patch.rect.maxY) * scale,
                                                  width: patch.rect.width * scale, height: patch.rect.height * scale))
        }
        return context.makeImage()
    }

    private mutating func add(_ raster: Raster, at offset: Offset) -> Bool {
        let tile = CGRect(x: offset.x, y: offset.y, width: raster.width, height: raster.height)
        let grown = bounds.union(tile)
        guard Self.fits(grown) else { return false }
        var uncovered = [tile]
        for patch in patches {
            uncovered = uncovered.flatMap { Self.subtract(patch.rect, from: $0) }
            if uncovered.isEmpty { break }
        }
        guard patches.count + uncovered.count <= Self.patchLimit else { return false }
        var added: [Patch] = []
        for rect in uncovered {
            let local = rect.offsetBy(dx: -tile.minX, dy: -tile.minY)
            guard let image = raster.image(in: local) else { return false }
            added.append(Patch(rect: rect, image: image))
        }
        patches += added
        bounds = grown
        last = raster
        position = offset
        return true
    }

    private static func fits(_ rect: CGRect) -> Bool {
        rect.width <= CGFloat(dimensionLimit) && rect.height <= CGFloat(dimensionLimit)
            && rect.width * rect.height <= CGFloat(pixelLimit)
    }

    private static func subtract(_ covered: CGRect, from rect: CGRect) -> [CGRect] {
        let overlap = rect.intersection(covered)
        guard !overlap.isNull, !overlap.isEmpty else { return [rect] }
        return [
            CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: overlap.minY - rect.minY),
            CGRect(x: rect.minX, y: overlap.maxY, width: rect.width, height: rect.maxY - overlap.maxY),
            CGRect(x: rect.minX, y: overlap.minY, width: overlap.minX - rect.minX, height: overlap.height),
            CGRect(x: overlap.maxX, y: overlap.minY, width: rect.maxX - overlap.maxX, height: overlap.height)
        ].filter { !$0.isEmpty }
    }

    private static func registration(from old: Raster, to new: Raster) -> Offset? {
        let stationary = score(.zero, from: old, to: new)
        if stationary.samples >= 6, stationary.quality >= 0.985 { return .zero }
        // Each proposed move is judged without the bands that stay put while the content moves that way.
        func scored(_ offset: Offset) -> Match {
            score(offset, from: old, to: new, fixed: fixedEdges(old, new, moved: offset))
        }

        var candidates = [stationary]
        // Matching scan lines make short stops at a scroll boundary precise, even
        // with long flat runs or repeating rows. Check every proposed overlap.
        for horizontal in [false, true] {
            candidates += offsets(from: old, to: new, horizontal: horizontal).map(scored)
        }
        if candidates.dropFirst().allSatisfy({ $0.quality < 0.97 || $0.matches < 26 }) {
            // Vision handles simultaneous panning and cases where exact scan line
            // signatures are disrupted by animation or a narrow fixed overlay.
            let request = VNTranslationalImageRegistrationRequest(targetedCGImage: new.original)
            if (try? VNImageRequestHandler(cgImage: old.original).perform([request])) != nil,
               let transform = request.results?.first?.alignmentTransform,
               transform.tx.isFinite, transform.ty.isFinite,
               abs(transform.tx) < CGFloat(old.width), abs(transform.ty) < CGFloat(old.height) {
                let x = Int(transform.tx.rounded())
                let y = Int(transform.ty.rounded())
                for offset in [Offset(x: x, y: -y), Offset(x: -x, y: y),
                               Offset(x: x, y: y), Offset(x: -x, y: -y)] where offset != .zero {
                    if !candidates.contains(where: { $0.offset == offset }) {
                        candidates.append(scored(offset))
                    }
                }
            }
        }
        let plausible = candidates.filter { $0.matches >= 9 && $0.quality >= 0.87 }.sorted(by: stronger)
        guard let best = plausible.first else { return nil }
        if best.offset == .zero { return .zero }
        guard best.quality > stationary.quality + 0.035 || stationary.samples < 9 else { return nil }
        let ambiguous = plausible.dropFirst().contains {
            $0.offset != best.offset && abs($0.quality - best.quality) < 0.025
                && Double($0.matches) >= Double(best.matches) * 0.8
        }
        return ambiguous ? nil : best.offset
    }

    private static func offsets(from old: Raster, to new: Raster, horizontal: Bool) -> [Offset] {
        let before = lineSignatures(old, horizontal: horizontal)
        let after = lineSignatures(new, horizontal: horizontal)
        var positions: [UInt64: [Int]] = [:]
        for i in 0..<(after.count - 5) where after[i] != after[i + 2] || after[i] != after[i + 5] {
            positions[lineKey(after, at: i), default: []].append(i)
        }
        var votes: [Int: Int] = [:]
        let maximum = before.count * 3 / 4
        for i in 0..<(before.count - 5) where before[i] != before[i + 2] || before[i] != before[i + 5] {
            guard let matches = positions[lineKey(before, at: i)], matches.count <= 24 else { continue }
            for j in matches where i != j && abs(i - j) <= maximum {
                votes[i - j, default: 0] += 1
            }
        }
        return votes.sorted { lhs, rhs in
            lhs.value == rhs.value ? abs(lhs.key) < abs(rhs.key) : lhs.value > rhs.value
        }.prefix(10).map { horizontal ? Offset(x: $0.key, y: 0) : Offset(x: 0, y: $0.key) }
    }

    private static func lineSignatures(_ raster: Raster, horizontal: Bool) -> [UInt64] {
        let lines = horizontal ? raster.width : raster.height
        let across = horizontal ? raster.height : raster.width
        let step = max(1, across / 28)
        let probes = Array(stride(from: step / 2, to: across, by: step))
        return (0..<lines).map { line in
            var signature = UInt64(14_695_981_039_346_656_037)
            for probe in probes {
                let index = horizontal ? probe * raster.width + line : line * raster.width + probe
                signature = (signature ^ UInt64(raster.pixels[index])) &* 1_099_511_628_211
            }
            return signature
        }
    }

    private static func lineKey(_ signatures: [UInt64], at line: Int) -> UInt64 {
        (signatures[line] &* 31 ^ signatures[line + 2]) &* 31 ^ signatures[line + 5]
    }

    private static func stronger(_ a: Match, _ b: Match) -> Bool {
        let aScore = a.quality * Double(min(a.matches, 40))
        let bScore = b.quality * Double(min(b.matches, 40))
        return aScore == bScore ? a.matches > b.matches : aScore > bScore
    }

    /// The rows along the top and the columns along the left that stay put while the content beside
    /// them moves by `moved`: a sticky header or a frozen column. Such a band matches only when
    /// nothing moved, so it is left out of the comparison. The first frame keeps the header in the
    /// image, and later frames place it over covered area.
    private static func fixedEdges(_ old: Raster, _ new: Raster, moved: Offset) -> Fixed {
        guard moved != .zero else { return .none }
        var fixed = Fixed.none
        func rows() {
            fixed.top = 0
            fixed.top = band(reach: old.height * 2 / 5) { index, end in
                fixed.top = end
                return line(old, new, moved, at: index, column: false, outside: fixed)
            }
        }
        rows()
        fixed.left = band(reach: old.width * 2 / 5) { index, end in
            fixed.left = end
            return line(old, new, moved, at: index, column: true, outside: fixed)
        }
        // A frozen column beside every row hides where the header ends until it is left out.
        if fixed.left > 0 { rows() }
        return fixed
    }

    /// How far a band reaches in from its edge: to the last line that stays put, before three lines
    /// that move with the content. Lines without detail neither end nor extend it. `line` gets each
    /// line's index and the band found so far.
    private static func band(reach: Int, _ line: (Int, Int) -> Line) -> Int {
        var end = 0
        var moving = 0
        for index in 0..<max(0, reach) where moving < 3 {
            switch line(index, end) {
            case .flat: continue
            case .still: end = index + 1; moving = 0
            case .moving: moving += 1
            }
        }
        return end
    }

    /// Whether one row (or column) of the view moves with the content: nine in ten of its detail
    /// pixels line up with the content where it moved (`moved`), in one frame or the other. A line
    /// whose pixels do not line up there stays put. Detail is counted only in the two pixels being
    /// compared, never in the same line of the other frame, so content faintly visible through a
    /// translucent header, and a moving scroll bar beside it, leave the header in its band, while
    /// short text on a white page still moves. Pixels in or moving into a band already found are left
    /// out: what moves under a header is hidden, not fixed.
    private static func line(_ old: Raster, _ new: Raster, _ moved: Offset, at line: Int, column: Bool,
                             outside fixed: Fixed) -> Line {
        let width = old.width
        let height = old.height
        let length = column ? height : width
        var samples = 0
        var matches = 0
        for along in stride(from: 0, to: length, by: max(1, length / 160)) {
            let x = column ? line : along
            let y = column ? along : line
            guard !fixed.covers(x, y) else { continue }
            // Where this old pixel's content is in the new frame.
            let (ax, ay) = (x - moved.x, y - moved.y)
            if ax >= 0, ax < width, ay >= 0, ay < height, !fixed.covers(ax, ay), clear(old, x, y) || clear(new, ax, ay) {
                samples += 1
                if same(old, x, y, new, ax, ay) { matches += 1 }
            }
            // Where this new pixel's content was in the old frame.
            let (bx, by) = (x + moved.x, y + moved.y)
            if bx >= 0, bx < width, by >= 0, by < height, !fixed.covers(bx, by), clear(new, x, y) || clear(old, bx, by) {
                samples += 1
                if same(new, x, y, old, bx, by) { matches += 1 }
            }
        }
        guard samples >= 4 else { return .flat }
        return matches * 10 >= samples * 9 ? .moving : .still
    }

    /// A pixel that clearly differs from its neighbor two rows down and two columns across. The faint
    /// edge of a letter is left out: it can look like a light header as much as like the page.
    private static func clear(_ raster: Raster, _ x: Int, _ y: Int) -> Bool {
        distance(raster.pixels[y * raster.width + x], neighbor(raster, x, y)) > 32
    }

    /// Two pixels and their neighbors match, as in score(): a white page beside a letter is not a
    /// light header beside nothing.
    private static func same(_ a: Raster, _ ax: Int, _ ay: Int, _ b: Raster, _ bx: Int, _ by: Int) -> Bool {
        near(a.pixels[ay * a.width + ax], b.pixels[by * b.width + bx]) && near(neighbor(a, ax, ay), neighbor(b, bx, by))
    }

    private static func neighbor(_ raster: Raster, _ x: Int, _ y: Int) -> UInt32 {
        let nx = x + 2 < raster.width ? x + 2 : x - 2
        let ny = y + 2 < raster.height ? y + 2 : y - 2
        return raster.pixels[ny * raster.width + nx]
    }

    /// The largest difference in any one color channel.
    private static func distance(_ a: UInt32, _ b: UInt32) -> Int {
        max(abs(Int(a & 255) - Int(b & 255)), abs(Int((a >> 8) & 255) - Int((b >> 8) & 255)),
            abs(Int((a >> 16) & 255) - Int((b >> 16) & 255)))
    }

    private static func score(_ offset: Offset, from old: Raster, to new: Raster, fixed: Fixed = .none,
                              density: Int = 32) -> Match {
        // Sample only where neither frame shows its fixed band.
        let left = max(fixed.left, fixed.left + offset.x)
        let top = max(0, offset.y)
        let right = min(old.width, old.width + offset.x) - 2
        let bottom = min(old.height, old.height + offset.y) - 2
        guard right - left >= max(6, old.width / 4 - 2),
              bottom - top >= max(6, old.height / 4 - 2) else { return Match(offset: offset, matches: 0, samples: 0) }
        let stepX = max(1, (right - left) / density)
        let stepY = max(1, (bottom - top) / density)
        // Evenly spaced rows can match the pitch of text lines and all land between them.
        // Golden-ratio spacing spreads the same number of rows over every part of a line.
        let rows = (bottom - top + stepY - 1) / stepY
        var matches = 0
        var samples = 0
        for row in 0..<rows {
            let y = top + Int((0.5 + Double(row) * 0.618_033_988_749_895)
                .truncatingRemainder(dividingBy: 1) * Double(bottom - top))
            if fixed.row(y) || fixed.row(y - offset.y) { continue }
            for x in stride(from: left, to: right, by: stepX) {
                let ox = x + y * old.width
                let nx = x - offset.x + (y - offset.y) * old.width
                let a = old.pixels[ox]
                let b = new.pixels[nx]
                let aEdge = old.pixels[ox + old.width * 2 + 2]
                let bEdge = new.pixels[nx + old.width * 2 + 2]
                // Flat areas such as a white page margin cannot establish an offset.
                if near(a, aEdge) && near(b, bEdge) { continue }
                samples += 1
                if near(a, b) && near(aEdge, bEdge) { matches += 1 }
            }
        }
        return Match(offset: offset, matches: matches, samples: samples)
    }

    private static func near(_ a: UInt32, _ b: UInt32) -> Bool {
        a == b || (
            abs(Int(a & 255) - Int(b & 255)) <= 8
                && abs(Int((a >> 8) & 255) - Int((b >> 8) & 255)) <= 8
                && abs(Int((a >> 16) & 255) - Int((b >> 16) & 255)) <= 8
        )
    }
}

private struct Raster {
    static let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    static let bitmapInfo = CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue
    let original: CGImage
    let width: Int
    let height: Int
    let pixels: [UInt32]

    init?(_ image: CGImage) {
        original = image
        width = image.width
        height = image.height
        let width = image.width
        let height = image.height
        var bytes = [UInt32](repeating: 0, count: width * height)
        let drew = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                          bytesPerRow: width * 4, space: Self.colorSpace, bitmapInfo: Self.bitmapInfo) else { return false }
            context.setBlendMode(.copy)
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drew else { return nil }
        pixels = bytes
    }

    func image(in rect: CGRect) -> CGImage? {
        let x = Int(rect.minX)
        let y = Int(rect.minY)
        let patchWidth = Int(rect.width)
        let patchHeight = Int(rect.height)
        // Store just the new pixels. A cropped CGImage can keep a full source frame alive.
        var bytes = Data(count: patchWidth * patchHeight * 4)
        bytes.withUnsafeMutableBytes { destination in
            pixels.withUnsafeBytes { source in
                guard let destination = destination.baseAddress, let source = source.baseAddress else { return }
                for row in 0..<patchHeight {
                    destination.advanced(by: row * patchWidth * 4)
                        .copyMemory(from: source.advanced(by: ((row + y) * width + x) * 4), byteCount: patchWidth * 4)
                }
            }
        }
        guard let provider = CGDataProvider(data: bytes as CFData) else { return nil }
        return CGImage(width: patchWidth, height: patchHeight, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: patchWidth * 4, space: Self.colorSpace, bitmapInfo: CGBitmapInfo(rawValue: Self.bitmapInfo),
                       provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }
}
