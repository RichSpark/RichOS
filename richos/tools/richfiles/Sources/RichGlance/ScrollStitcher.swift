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
        var fixed = Fixed.none

        var quality: Double { samples == 0 ? 0 : Double(matches) / Double(samples) }
    }

    /// Bands along the edges of a view `width` × `height` that stay put between two frames while the
    /// content between them moves: a sticky header, a bottom bar, a frozen column, a side bar.
    private struct Fixed: Equatable {
        var top = 0
        var bottom = 0
        var left = 0
        var right = 0
        var width = 0
        var height = 0
        static let none = Fixed()

        func row(_ y: Int) -> Bool { y < top || (bottom > 0 && y >= height - bottom) }
        func covers(_ x: Int, _ y: Int) -> Bool { row(y) || x < left || (right > 0 && x >= width - right) }
    }

    private struct Patch {
        let rect: CGRect
        let image: CGImage
        /// The place in the stitched image of the frame these pixels came from.
        let tile: CGRect
        /// For the pixels of a frame's fixed bands, the sides that have shown a bar (0 top, 1 bottom,
        /// 2 left, 3 right) in whose band they lie; nil for content. They are drawn only where no
        /// frame showed the content under them.
        var sides: [Int]?
        var bar: Bool { sides != nil }

        /// Whether a piece of a band from a frame at `tile`, in the bands of `sides`, shows the page
        /// where this earlier piece may show a bar: it lies in no band this one is not in, and its
        /// frame is at least as far out on each of those sides, with fewer bands or farther out on
        /// one. Under a bottom bar the page is seen only from a frame lower down, which shows it
        /// higher in its view; so for every side.
        func yields(to sides: [Int], at tile: CGRect) -> Bool {
            guard let own = self.sides, sides.allSatisfy(own.contains) else { return false }
            let gains = sides.map { side -> CGFloat in
                switch side {
                case 0: return self.tile.minY - tile.minY
                case 1: return tile.maxY - self.tile.maxY
                case 2: return self.tile.minX - tile.minX
                default: return tile.maxX - self.tile.maxX
                }
            }
            return gains.allSatisfy { $0 >= 0 } && (sides.count < own.count || gains.contains { $0 > 0 })
        }
    }

    static let pixelLimit = 48_000_000
    static let dimensionLimit = 32_000
    private static let patchLimit = 6_000
    private var last: Raster?
    private var position = Offset.zero
    private var patches: [Patch] = []
    /// The first frame is placed whole until a move shows which of its edges are fixed bars.
    private var settled = false
    /// The fixed bars the last move found.
    private var bars: Fixed?
    /// The sides (top, bottom, left, right) where a move has shown a bar that stays put.
    private var barred = [false, false, false, false]
    /// For each side, the narrowest band a move along its axis left without moving content; nil
    /// until such a move. A bar is no larger: a line that moved with the page once is page.
    private var most: [Int?] = [nil, nil, nil, nil]
    /// For each side, the widest band a move across its axis left without moving content, which
    /// stands in until a move along it.
    private var across = [0, 0, 0, 0]
    /// The bands every frame so far is placed by, and the sides that had shown a bar then.
    private var placed: (bands: [Int], barred: [Bool])?
    private(set) var bounds = CGRect.null

    /// The share of the view's width and height that moves with the content between the fixed bars
    /// the last move found; nil before the first move.
    var movingShare: CGSize? {
        guard let bars, let last else { return nil }
        return CGSize(width: Double(last.width - bars.left - bars.right) / Double(last.width),
                      height: Double(last.height - bars.top - bars.bottom) / Double(last.height))
    }

    /// How far one automatic scroll step goes along a view `length` long. The first step is short, to
    /// find any fixed bars; after it a step keeps at least half of the content between the bars
    /// (`movingShare`) in view, so the next frame still overlaps it, and never goes past 28% of the
    /// view, as before.
    static func automaticStep(along length: Double, moving share: Double?) -> Double {
        max(4, min(220, length * (share.map { min(0.28, $0 * 0.5) } ?? 0.1)))
    }

    mutating func append(_ image: CGImage) -> Update {
        guard image.width >= 24, image.height >= 24,
              Self.fits(CGRect(x: 0, y: 0, width: image.width, height: image.height)),
              let next = Raster(image) else { return .limit }
        if let last {
            guard next.width == last.width, next.height == last.height else { return .unmatched }
            if next.pixels == last.pixels { return .unchanged }
            guard let match = Self.registration(from: last, to: next) else { return .unmatched }
            let offset = match.offset
            guard offset != .zero else { return .unchanged }
            let wide = Self.widened(match.fixed, from: last, to: next, moved: offset)
            let narrow = [match.fixed.top, match.fixed.bottom, match.fixed.left, match.fixed.right]
            var bands = [wide.top, wide.bottom, wide.left, wide.right]
            for side in 0..<4 {
                if narrow[side] > 0 { barred[side] = true }
                // A sideways move cannot tell a see-through header from the page moving under it, so
                // each band is learned from the moves along its own axis, and from all of them: beside
                // a bar, page that is blank in this move is still page once another move showed it
                // moving. Until such a move the widest band seen stands in.
                if (side < 2 ? offset.y : offset.x) != 0 {
                    most[side] = min(most[side] ?? bands[side], bands[side])
                } else {
                    across[side] = max(across[side], bands[side])
                }
                bands[side] = most[side] ?? across[side]
            }
            var fixed = wide
            (fixed.top, fixed.bottom, fixed.left, fixed.right) = (bands[0], bands[1], bands[2], bands[3])
            if !settled {
                patches = []
                bounds = .null
                guard add(last, at: position, fixed: fixed) else { return .limit }
                settled = true
            } else if let placed, placed.bands != bands || placed.barred != barred {
                // Every frame is placed by the same bands: what was learned now holds for earlier ones.
                guard sort(by: fixed) else { return .limit }
            }
            placed = (bands, barred)
            let nextPosition = Offset(x: position.x + offset.x, y: position.y + offset.y)
            guard add(next, at: nextPosition, fixed: fixed) else { return .limit }
            // Steps are sized by the bars themselves, not by the blank page widened into them.
            bars = match.fixed
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

    /// The bands of `fixed` in a frame at `tile` as up to eight pieces around its content, each with
    /// the sides that have shown a bar in whose band it lies and the side it is on.
    private func pieces(of tile: CGRect, fixed: Fixed) -> (content: CGRect, bars: [(rect: CGRect, sides: [Int], side: Int)]) {
        let xs = [tile.minX, tile.minX + CGFloat(fixed.left), tile.maxX - CGFloat(fixed.right), tile.maxX]
        let ys = [tile.minY, tile.minY + CGFloat(fixed.top), tile.maxY - CGFloat(fixed.bottom), tile.maxY]
        var bars: [(rect: CGRect, sides: [Int], side: Int)] = []
        for row in 0..<3 {
            for column in 0..<3 where row != 1 || column != 1 {
                let rect = CGRect(x: xs[column], y: ys[row], width: xs[column + 1] - xs[column], height: ys[row + 1] - ys[row])
                guard !rect.isEmpty else { continue }
                let sides = ((row == 1 ? [] : [row / 2]) + (column == 1 ? [] : [2 + column / 2])).filter { barred[$0] }
                let side = row == 1 ? 2 + column / 2 : row / 2
                if let before = bars.last, before.sides == sides, before.side == side, before.rect.minY == rect.minY,
                   before.rect.maxX == rect.minX {
                    bars[bars.count - 1].rect = before.rect.union(rect)
                } else {
                    bars.append((rect, sides, side))
                }
            }
        }
        return (CGRect(x: xs[1], y: ys[1], width: xs[2] - xs[1], height: ys[2] - ys[1]), bars)
    }

    /// Place a frame's content where no earlier frame showed content, and its fixed bars (`fixed`)
    /// only where nothing else is: content always replaces a bar. A bar on a side where this frame
    /// reaches past everything captured so far replaces the bars of earlier frames, so a bottom bar
    /// ends up once, below the last content; any other bar stays behind them, so a header stays once
    /// above the first content, and scrolling back and forth near an end leaves its bar whole. A
    /// piece of a band also replaces an earlier one where it is the likelier to show the page
    /// (`Patch.yields`): a blank page margin replaces the end of an earlier bottom bar, and the
    /// page beside a tool bar replaces the bar an earlier frame showed there, never the reverse.
    private mutating func add(_ raster: Raster, at offset: Offset, fixed: Fixed = .none) -> Bool {
        let tile = CGRect(x: offset.x, y: offset.y, width: raster.width, height: raster.height)
        let grown = bounds.union(tile)
        guard Self.fits(grown) else { return false }
        let leads = [tile.minY < bounds.minY, tile.maxY > bounds.maxY, tile.minX < bounds.minX, tile.maxX > bounds.maxX]
            .map { !bounds.isNull && $0 }
        let (content, bars) = pieces(of: tile, fixed: fixed)
        let solid = patches.filter { !$0.bar }.map(\.rect)
        var fresh: [(CGRect, [Int]?)] = content.isEmpty ? [] : Self.subtract(solid, from: [content]).map { ($0, nil) }
        guard var kept = Self.trim(patches, by: content) else { return false }
        for bar in bars {
            guard let under = Self.trim(kept, by: bar.rect, where: { leads[bar.side] || $0.yields(to: bar.sides, at: tile) })
            else { return false }
            kept = under
            fresh += Self.subtract(kept.filter(\.bar).map(\.rect), from: Self.subtract(solid, from: [bar.rect])).map { ($0, bar.sides) }
        }
        guard kept.count + fresh.count <= Self.patchLimit else { return false }
        var added: [Patch] = []
        for (rect, sides) in fresh {
            guard let image = raster.image(in: rect.offsetBy(dx: -tile.minX, dy: -tile.minY)) else { return false }
            added.append(Patch(rect: rect, image: image, tile: tile, sides: sides))
        }
        patches = kept + added
        bounds = grown
        last = raster
        position = offset
        return true
    }

    /// Every pixel placed so far as content or as bar by the bands of `fixed` in the frame it came
    /// from: content in a band becomes bar, which stays where nothing else is and which any frame
    /// that shows content there replaces; bar outside every band becomes content.
    private mutating func sort(by fixed: Fixed) -> Bool {
        var result: [Patch] = []
        for patch in patches {
            let (content, bars) = pieces(of: patch.tile, fixed: fixed)
            for (rect, sides) in [(content, nil)] + bars.map({ ($0.rect, Optional($0.sides)) }) {
                let part = rect.intersection(patch.rect)
                guard !part.isNull, !part.isEmpty else { continue }
                guard let image = patch.image.cropping(to: part.offsetBy(dx: -patch.rect.minX, dy: -patch.rect.minY)) else { return false }
                result.append(Patch(rect: part, image: image, tile: patch.tile, sides: sides))
            }
        }
        guard result.count <= Self.patchLimit else { return false }
        patches = result
        return true
    }

    /// Earlier bars, all or those `where` says, cut back so that `rect` is free of them.
    private static func trim(_ patches: [Patch], by rect: CGRect, where goes: (Patch) -> Bool = { _ in true }) -> [Patch]? {
        var result: [Patch] = []
        for patch in patches {
            guard patch.bar, goes(patch), patch.rect.intersects(rect) else { result.append(patch); continue }
            for piece in subtract(rect, from: patch.rect) {
                let local = piece.offsetBy(dx: -patch.rect.minX, dy: -patch.rect.minY)
                guard let image = patch.image.cropping(to: local) else { return nil }
                result.append(Patch(rect: piece, image: image, tile: patch.tile, sides: patch.sides))
            }
        }
        return result
    }

    private static func subtract(_ covered: [CGRect], from rects: [CGRect]) -> [CGRect] {
        var left = rects
        for rect in covered {
            left = left.flatMap { subtract(rect, from: $0) }
            if left.isEmpty { break }
        }
        return left
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

    private static func registration(from old: Raster, to new: Raster) -> Match? {
        let stationary = score(.zero, from: old, to: new)
        if stationary.samples >= 6, stationary.quality >= 0.985 { return stationary }
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
        if best.offset == .zero { return best }
        guard best.quality > stationary.quality + 0.035 || stationary.samples < 9 else { return nil }
        let ambiguous = plausible.dropFirst().contains {
            $0.offset != best.offset && abs($0.quality - best.quality) < 0.025
                && Double($0.matches) >= Double(best.matches) * 0.8
        }
        return ambiguous ? nil : best
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

    /// The rows along the top and bottom and the columns along the left and right that stay put
    /// while the content between them moves by `moved`: a sticky header, a bottom bar, a frozen
    /// column, a side bar. Such a band matches only when nothing moved, so it is left out of the
    /// comparison. Rows are found first across the columns, then columns between those rows, then rows
    /// again beside any side bar.
    private static func fixedEdges(_ old: Raster, _ new: Raster, moved: Offset) -> Fixed {
        guard moved != .zero else { return .none }
        var fixed = Fixed(width: old.width, height: old.height)
        func rows() {
            (fixed.top, fixed.bottom) = (0, 0)
            let across = fixed
            (fixed.top, fixed.bottom) = edges(old.height, moved.y) { differs(old, new, moved, line: $0, column: false, outside: across) }
        }
        rows()
        let across = fixed
        (fixed.left, fixed.right) = edges(old.width, moved.x) { differs(old, new, moved, line: $0, column: true, outside: across) }
        if fixed.left + fixed.right > 0 { rows() }
        return fixed
    }

    /// The narrowest bands at the start and the end of `length` lines that hold at least one line of
    /// every pair that does not line up: old line i and new line i - `shift`, where the content moved.
    /// Lines of a fixed bar never line up with the content beside them, so each such pair has a line in
    /// a bar; a translucent header's own rows can line up with each other while the content under them
    /// moves, and need not. A band may fill half the view, as a site header does in a short selection.
    private static func edges(_ length: Int, _ shift: Int, _ differs: (Int) -> Bool) -> (start: Int, end: Int) {
        let reach = length / 2
        var pairs: [(low: Int, high: Int)] = []
        for line in max(0, shift)..<min(length, length + shift) {
            let low = min(line, line - shift)
            let high = max(line, line - shift)
            if low < reach || high >= length - reach, differs(line) { pairs.append((low, high)) }
        }
        guard !pairs.isEmpty else { return (0, 0) }
        pairs.sort { $0.low < $1.low }
        // The end band each start band leaves to hold: every pair whose low line it does not reach.
        var needed = [Int](repeating: 0, count: pairs.count + 1)
        for index in pairs.indices.reversed() { needed[index] = max(needed[index + 1], length - pairs[index].high) }
        var best = (start: reach, end: reach)
        var next = 0
        for start in [0] + pairs.map({ $0.low + 1 }) where start <= reach {
            while next < pairs.count, pairs[next].low < start { next += 1 }
            if needed[next] <= reach, start + needed[next] < best.start + best.end { best = (start, needed[next]) }
        }
        return best
    }

    /// The bands of `fixed`, each widened over the lines beside it that do not show moving content:
    /// up to the first line that changed where it is and lines up with the line the move `moved`
    /// took it to or brought it from, on the side away from the edge. The blank margin of a
    /// bar, a faint divider and blank page beside them cannot be told apart, so all of them are kept
    /// as bar, which any frame that shows content there replaces; kept as content, a bar's margin
    /// would stay over the page for good. The registration itself uses the narrow bands.
    private static func widened(_ fixed: Fixed, from old: Raster, to new: Raster, moved: Offset) -> Fixed {
        var wide = fixed
        (wide.width, wide.height) = (old.width, old.height)
        func grow(_ start: Int, _ end: Int, _ length: Int, _ shift: Int, column: Bool) -> (Int, Int) {
            let across = wide
            let reach = length / 2
            var (start, end) = (start, end)
            func moving(_ line: Int, _ partner: Int) -> Bool {
                moves(old, new, moved, line: line, partner: partner, column: column, outside: across)
            }
            while start < reach, !moving(start, start + abs(shift)) { start += 1 }
            while end < reach, !moving(length - 1 - end, length - 1 - end - abs(shift)) { end += 1 }
            return (start, end)
        }
        (wide.top, wide.bottom) = grow(wide.top, wide.bottom, old.height, moved.y, column: false)
        (wide.left, wide.right) = grow(wide.left, wide.right, old.width, moved.x, column: true)
        return wide
    }

    /// Whether view line `line` shows moving content: at least eight of its pixels clearly changed in
    /// place between the two frames, and three quarters of those are, to within two levels, the
    /// pixels of the line `partner` in the other frame, where the move took them: page seen through
    /// a tinted bar is not. Pixels in a band of `outside` are left out.
    private static func moves(_ old: Raster, _ new: Raster, _ moved: Offset, line: Int, partner: Int, column: Bool,
                              outside fixed: Fixed) -> Bool {
        let shift = column ? moved.x : moved.y
        guard partner >= 0, partner < (column ? old.width : old.height) else { return false }
        // Old line i lines up with new line i - shift.
        let first = line - shift == partner ? line : partner
        let length = column ? old.height : old.width
        let slide = column ? moved.y : moved.x
        var samples = 0
        var matches = 0
        for along in max(0, slide)..<min(length, length + slide) {
            let (x, y) = column ? (first, along) : (along, first)
            let (nx, ny) = (x - moved.x, y - moved.y)
            guard !fixed.covers(x, y), !fixed.covers(nx, ny) else { continue }
            // The pixels themselves, without the neighbors the other comparisons use: a neighbor two
            // lines on can be moving page while the line is the last one of a bar.
            let place = first == line ? y * old.width + x : ny * old.width + nx
            guard distance(old.pixels[place], new.pixels[place]) > 32 else { continue }
            samples += 1
            if distance(old.pixels[y * old.width + x], new.pixels[ny * old.width + nx]) <= 2 { matches += 1 }
        }
        return samples >= 8 && matches * 4 >= samples * 3
    }

    /// Whether old line `line` and the new line where its content moved do not line up: fewer than
    /// half of their clearly detailed pixels match, leaving out pixels in a band of `outside`. A pair
    /// without detail is not evidence either way.
    private static func differs(_ old: Raster, _ new: Raster, _ moved: Offset, line: Int, column: Bool,
                                outside fixed: Fixed) -> Bool {
        let length = column ? old.height : old.width
        let shift = column ? moved.y : moved.x
        var samples = 0
        var matches = 0
        for along in stride(from: max(0, shift), to: min(length, length + shift), by: max(1, length / 160)) {
            let (x, y) = column ? (line, along) : (along, line)
            let (nx, ny) = (x - moved.x, y - moved.y)
            // Within two pixels of the far edges a pixel's neighbor lies the other way, and never matches.
            guard max(x, nx) + 2 < old.width, max(y, ny) + 2 < old.height else { continue }
            guard !fixed.covers(x, y), !fixed.covers(nx, ny), clear(old, x, y) || clear(new, nx, ny) else { continue }
            samples += 1
            if same(old, x, y, new, nx, ny) { matches += 1 }
        }
        return samples >= 4 && matches * 2 < samples
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
        let right = min(old.width - fixed.right, old.width - fixed.right + offset.x) - 2
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
        return Match(offset: offset, matches: matches, samples: samples, fixed: fixed)
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
