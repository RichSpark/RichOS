import CoreGraphics
import Foundation
import Vision

/// Registration proposes an integer translation. It cannot grant permission to export:
/// every multiply observed page pixel must agree exactly at that translation.
struct ScrollStitcher {
    enum Update: Equatable {
        case initial, unchanged, moved(dx: Int, dy: Int), unmatched, conflict, limit
    }

    private struct Offset: Hashable {
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

    private struct Fixed: Equatable {
        var top = 0
        var bottom = 0
        var left = 0
        var right = 0
        var width = 0
        var height = 0
        static let none = Fixed()

        static let sides = [\Fixed.top, \Fixed.bottom, \Fixed.left, \Fixed.right]
        var hasEdges: Bool { top + bottom + left + right > 0 }

        func row(_ y: Int) -> Bool { y < top || (bottom > 0 && y >= height - bottom) }
        func covers(_ x: Int, _ y: Int) -> Bool { row(y) || x < left || (right > 0 && x >= width - right) }
        func body(_ raster: Raster, at origin: Offset) -> CGRect {
            CGRect(x: origin.x + left, y: origin.y + top,
                   width: raster.width - left - right, height: raster.height - top - bottom)
        }
    }

    private struct Frame {
        let raster: Raster
        let origin: Offset

        var viewport: CGRect { Fixed.none.body(raster, at: origin) }
    }

    private struct Row {
        let x: Int
        // nil means no frame showed this place, as at the corner of a diagonal pan.
        var pixels: ContiguousArray<UInt32?>
    }

    static let pixelLimit = 48_000_000
    static let dimensionLimit = 32_000
    private var last: Frame?
    private var fixed: Fixed?
    private var rows: [Int: Row] = [:]
    private var origins: Set<Offset> = []
    private var top: Frame?
    private var bottom: Frame?
    private var left: Frame?
    private var right: Frame?
    private(set) var issue: Update?
    private(set) var bounds = CGRect.null

    var movingShare: CGSize? {
        guard let fixed, let last else { return nil }
        let body = fixed.body(last.raster, at: last.origin)
        return CGSize(width: body.width / last.viewport.width, height: body.height / last.viewport.height)
    }

    static func automaticStep(along length: Double, moving share: Double?) -> Double {
        max(4, min(220, length * (share.map { min(0.28, $0 * 0.5) } ?? 0.1)))
    }

    mutating func append(_ image: CGImage, final: Bool = false, resampling: Bool = false) -> Update {
        guard image.width >= 24, image.height >= 24,
              Self.fits(CGRect(x: 0, y: 0, width: image.width, height: image.height)),
              let next = Raster(image) else { return refuse(.limit) }
        guard let last else {
            let first = Frame(raster: next, origin: .zero)
            keep(first)
            return .initial
        }
        guard next.width == last.raster.width, next.height == last.raster.height else { return refuse(.conflict) }
        if next.pixels == last.raster.pixels {
            issue = nil
            return .unchanged
        }
        // An unplaced frame adds no pixels. A later view can rejoin the verified page
        // through exact overlap. A known conflict requires returning to the last view.
        if issue == .conflict { return .conflict }
        if issue == nil, final, let fixed, fixed.hasEdges, origins.count >= 3 {
            let settled = Frame(raster: next, origin: last.origin)
            if finalRepaintAgrees(last, settled, fixed: fixed) {
                keep(settled)
                return .unchanged
            }
        }
        guard let offset = Self.registration(from: last.raster, to: next)?.offset else {
            return resampling ? .unmatched : refuse(.unmatched)
        }
        guard offset != .zero else { return refuse(.conflict) }
        let frame = Frame(raster: next, origin: Offset(x: last.origin.x + offset.x, y: last.origin.y + offset.y))
        guard Self.fits(bounds.union(frame.viewport)) else { return refuse(.limit) }

        let layout: Fixed
        if let fixed {
            var narrower = fixed
            if !Self.fixedPixelsAgree(last.raster, next, fixed) {
                let stationary = Self.stationaryEdges(last.raster, next)
                for side in Fixed.sides {
                    narrower[keyPath: side] = min(fixed[keyPath: side], stationary[keyPath: side])
                }
            }
            let overlap = Self.overlap(last, frame, excluding: narrower)
            guard overlap.width >= 6, overlap.height >= 6 else {
                return resampling ? .unmatched : refuse(.unmatched)
            }
            for (index, side) in Fixed.sides.enumerated() where narrower[keyPath: side] > 0 {
                let perpendicular = index < 2 ? offset.x != 0 : offset.y != 0
                if perpendicular && !Self.visibleSeam(in: next, side: index, depth: narrower[keyPath: side]) {
                    return refuse(.conflict)
                }
            }
            if narrower != fixed {
                // All previously masked pixels were identical on screen. Replay those actual
                // observations before exposing any of them as page. Never enlarge a mask to
                // explain away a new conflict.
                var trial = self
                guard trial.restoreEdges(from: last.raster, old: fixed, new: narrower),
                      trial.agrees(frame, in: narrower.body(next, at: frame.origin)) else {
                    return refuse(.conflict)
                }
                self = trial
            } else if !agrees(frame, in: fixed.body(next, at: frame.origin)) {
                return refuse(.conflict)
            }
            layout = narrower
        } else {
            guard let verified = Self.verifyFirstMove(from: last, to: frame) else { return refuse(.conflict) }
            layout = verified
            keepBody(last, in: layout.body(last.raster, at: last.origin))
        }
        fixed = layout
        keepBody(frame, in: layout.body(next, at: frame.origin))
        keep(frame)
        issue = nil
        return .moved(dx: offset.x, dy: offset.y)
    }

    private mutating func refuse(_ update: Update) -> Update {
        if issue != .conflict { issue = update }
        return update
    }

    /// Longest completely unchanged strips touching each screen edge. Fuzzy similarity, blending
    /// and a stationary object in the middle of moving content cannot establish a fixed strip.
    private static func stationaryEdges(_ old: Raster, _ new: Raster) -> Fixed {
        var minX = old.width
        var minY = old.height
        var maxX = -1
        var maxY = -1
        old.pixels.withUnsafeBufferPointer { before in
            new.pixels.withUnsafeBufferPointer { after in
                for y in 0..<old.height {
                    let start = y * old.width
                    for x in 0..<old.width where before[start + x] != after[start + x] {
                        minX = min(x, minX)
                        maxX = max(x, maxX)
                        minY = min(y, minY)
                        maxY = y
                    }
                    // No edge remains stationary. The rest cannot affect these bounds.
                    if minX == 0, maxX == old.width - 1, minY == 0, maxY == old.height - 1 { break }
                }
            }
        }
        return Fixed(top: minY, bottom: old.height - maxY - 1, left: minX, right: old.width - maxX - 1,
                     width: old.width, height: old.height)
    }

    private static func verifyFirstMove(from old: Frame, to new: Frame) -> Fixed? {
        if overlapAgrees(old, new, excluding: .none) { return Fixed.none }
        let stationary = stationaryEdges(old.raster, new.raster)
        var choices = [Fixed.none]
        for (index, edge) in Fixed.sides.enumerated() {
            let extent = stationary[keyPath: edge]
            guard extent > 0 else { continue }
            let perpendicular = index < 2 ? old.origin.x != new.origin.x : old.origin.y != new.origin.y
            let boundary = (1...extent).reversed().first { visibleSeam(in: old.raster, side: index, depth: $0) }
            // An entire strip beside the scroll is drawn only once. Require a visible separation
            // so a fixed corner button cannot turn a blank page margin into a missing sidebar.
            guard let depth = boundary ?? (perpendicular ? nil : extent) else { continue }
            choices += choices.map { choice in
                var withEdge = choice
                withEdge[keyPath: edge] = depth
                return withEdge
            }
        }
        let matching = choices.dropFirst().filter { overlapAgrees(old, new, excluding: $0) }
        // If two different minimal sets of edges explain the conflict, the layout is ambiguous.
        let minimal = matching.filter { mask in
            !matching.contains { other in
                other != mask && Fixed.sides.allSatisfy { other[keyPath: $0] <= mask[keyPath: $0] }
            }
        }
        return minimal.count == 1 ? minimal.first : nil
    }

    private static func visibleSeam(in raster: Raster, side: Int, depth: Int) -> Bool {
        let column = side >= 2
        let leading = side.isMultiple(of: 2)
        let length = column ? raster.width : raster.height
        guard depth > 0, depth < length else { return false }
        let across = column ? raster.height : raster.width
        let boundary = leading ? depth : length - depth
        let strideInRaster = column ? 1 : raster.width
        var contrasting = 0
        for cross in 0..<across {
            let index = column ? cross * raster.width + boundary : boundary * raster.width + cross
            if distance(raster.pixels[index], raster.pixels[index - strideInRaster]) > 8 { contrasting += 1 }
        }
        return contrasting * 4 >= across * 3
    }

    private static func fixedPixelsAgree(_ old: Raster, _ new: Raster, _ fixed: Fixed) -> Bool {
        guard fixed.hasEdges else { return true }
        return old.pixels.withUnsafeBufferPointer { before in
            new.pixels.withUnsafeBufferPointer { after in
                for y in 0..<old.height {
                    let start = y * old.width
                    let end = start + old.width
                    if y < fixed.top || y >= old.height - fixed.bottom {
                        if !before[start..<end].elementsEqual(after[start..<end]) { return false }
                    } else {
                        let leading = start..<(start + fixed.left)
                        let trailing = (end - fixed.right)..<end
                        if !before[leading].elementsEqual(after[leading])
                            || !before[trailing].elementsEqual(after[trailing]) { return false }
                    }
                }
                return true
            }
        }
    }

    private static func overlap(_ old: Frame, _ new: Frame, excluding fixed: Fixed) -> CGRect {
        fixed.body(old.raster, at: old.origin).intersection(fixed.body(new.raster, at: new.origin))
    }

    private static func overlapAgrees(_ old: Frame, _ new: Frame, excluding fixed: Fixed) -> Bool {
        let common = overlap(old, new, excluding: fixed)
        guard !common.isNull, common.width >= 6, common.height >= 6 else { return false }
        let (x, width) = (Int(common.minX), Int(common.width))
        return old.raster.pixels.withUnsafeBufferPointer { before in
            new.raster.pixels.withUnsafeBufferPointer { after in
                for y in Int(common.minY)..<Int(common.maxY) {
                    let oldStart = (y - old.origin.y) * old.raster.width + x - old.origin.x
                    let newStart = (y - new.origin.y) * new.raster.width + x - new.origin.x
                    if !before[oldStart..<(oldStart + width)].elementsEqual(after[newStart..<(newStart + width)]) {
                        return false
                    }
                }
                return true
            }
        }
    }

    private func agrees(_ frame: Frame, in body: CGRect) -> Bool {
        return frame.raster.pixels.withUnsafeBufferPointer { source in
            for y in Int(body.minY)..<Int(body.maxY) {
                guard let row = rows[y] else { continue }
                let start = max(row.x, Int(body.minX))
                let end = min(row.x + row.pixels.count, Int(body.maxX))
                guard start < end else { continue }
                let offset = (y - frame.origin.y) * frame.raster.width - frame.origin.x
                for x in start..<end {
                    if let old = row.pixels[x - row.x], old != source[offset + x] { return false }
                }
            }
            return true
        }
    }

    private mutating func keepBody(_ frame: Frame, in body: CGRect) {
        let (start, end) = (Int(body.minX), Int(body.maxX))
        frame.raster.pixels.withUnsafeBufferPointer { source in
            for y in Int(body.minY)..<Int(body.maxY) {
                let offset = (y - frame.origin.y) * frame.raster.width - frame.origin.x
                var row = rows.removeValue(forKey: y) ?? Row(x: start, pixels: [])
                let rowEnd = row.x + row.pixels.count
                if start >= row.x {
                    row.pixels.append(contentsOf: repeatElement(nil, count: max(0, end - rowEnd)))
                } else {
                    let low = min(start, row.x)
                    let high = max(end, rowEnd)
                    var grown = ContiguousArray<UInt32?>(repeating: nil, count: high - low)
                    grown.replaceSubrange((row.x - low)..<(rowEnd - low), with: row.pixels)
                    row = Row(x: low, pixels: grown)
                }
                for x in start..<end { row.pixels[x - row.x] = source[offset + x] }
                rows[y] = row
            }
        }
    }

    private mutating func restoreEdges(from raster: Raster, old: Fixed, new: Fixed) -> Bool {
        let origins = origins
        for origin in origins {
            let former = old.body(raster, at: origin)
            let body = new.body(raster, at: origin)
            let strips = [
                CGRect(x: body.minX, y: body.minY, width: body.width, height: former.minY - body.minY),
                CGRect(x: body.minX, y: former.maxY, width: body.width, height: body.maxY - former.maxY),
                CGRect(x: body.minX, y: former.minY, width: former.minX - body.minX, height: former.height),
                CGRect(x: former.maxX, y: former.minY, width: body.maxX - former.maxX, height: former.height)
            ]
            let frame = Frame(raster: raster, origin: origin)
            for strip in strips where !strip.isEmpty {
                guard agrees(frame, in: strip) else { return false }
                keepBody(frame, in: strip)
            }
        }
        return true
    }

    private mutating func keep(_ frame: Frame) {
        let (x, y) = (frame.origin.x, frame.origin.y)
        if top.map({ (y, x) <= ($0.origin.y, $0.origin.x) }) ?? true { top = frame }
        if bottom.map({ (-y, x) <= (-$0.origin.y, $0.origin.x) }) ?? true { bottom = frame }
        if left.map({ (x, y) <= ($0.origin.x, $0.origin.y) }) ?? true { left = frame }
        if right.map({ (-x, y) <= (-$0.origin.x, $0.origin.y) }) ?? true { right = frame }
        bounds = bounds.union(frame.viewport)
        origins.insert(frame.origin)
        last = frame
    }

    private func edgeTiles(_ fixed: Fixed, in raster: Raster) -> [(Frame?, CGRect)] {
        [
            (top, CGRect(x: 0, y: 0, width: raster.width, height: fixed.top)),
            (bottom, CGRect(x: 0, y: raster.height - fixed.bottom, width: raster.width, height: fixed.bottom)),
            (left, CGRect(x: 0, y: 0, width: fixed.left, height: raster.height)),
            (right, CGRect(x: raster.width - fixed.right, y: 0, width: fixed.right, height: raster.height))
        ]
    }

    private func finalRepaintAgrees(_ old: Frame, _ settled: Frame, fixed: Fixed) -> Bool {
        // Every changed pixel must be in a proven edge that this viewport actually contributes to
        // the result. The body and edges contributed from other positions still have to agree.
        let edges = edgeTiles(fixed, in: settled.raster).compactMap { owner, edge in
            owner?.origin == settled.origin && !edge.isEmpty ? edge : nil
        }
        guard !edges.isEmpty else { return false }
        let width = settled.raster.width
        return old.raster.pixels.withUnsafeBufferPointer { before in
            settled.raster.pixels.withUnsafeBufferPointer { after in
                for index in before.indices where before[index] != after[index] {
                    if !edges.contains(where: { $0.contains(CGPoint(x: index % width, y: index / width)) }) {
                        return false
                    }
                }
                return true
            }
        }
    }

    func image(previewLimit: Int? = nil, requireComplete: Bool = false) -> CGImage? {
        guard issue == nil, let last else { return nil }
        guard let fixed else { return previewLimit == nil ? last.raster.original : nil }
        // Two views can identify an overlap. A fixed edge also needs an independent position.
        guard previewLimit != nil || !fixed.hasEdges || origins.count >= 3 else { return nil }
        let (minX, minY, width, height) = (Int(bounds.minX), Int(bounds.minY), Int(bounds.width), Int(bounds.height))

        // Each bar is drawn from a single original viewport on its own outside edge. In particular,
        // a sidebar beside a long vertical capture never contributes more copies of its controls.
        let bars = edgeTiles(fixed, in: last.raster)
        func edgePixel(x: Int, y: Int) -> UInt32? {
            for (owner, rect) in bars {
                guard let owner else { continue }
                let (column, row) = (x - owner.origin.x, y - owner.origin.y)
                if rect.contains(CGPoint(x: column, y: row)) {
                    return owner.raster.pixels[row * owner.raster.width + column]
                }
            }
            return nil
        }

        let scale = min(1, Double(previewLimit ?? max(width, height)) / Double(max(width, height)))
        let outWidth = max(1, Int((Double(width) * scale).rounded(.up)))
        let outHeight = max(1, Int((Double(height) * scale).rounded(.up)))
        let page = CGRect(x: minX + fixed.left, y: minY + fixed.top,
                          width: width - fixed.left - fixed.right, height: height - fixed.top - fixed.bottom)
        var data = Data(count: outWidth * outHeight * 4)
        let complete = data.withUnsafeMutableBytes { buffer -> Bool in
            let pixels = buffer.bindMemory(to: UInt32.self)
            for y in 0..<outHeight {
                let row = minY + y * height / outHeight
                let body = rows[row]
                for x in 0..<outWidth {
                    let column = minX + x * width / outWidth
                    if let body, body.pixels.indices.contains(column - body.x), let value = body.pixels[column - body.x] {
                        pixels[y * outWidth + x] = value
                    } else if requireComplete, page.contains(CGPoint(x: column, y: row)) {
                        return false
                    } else if let value = edgePixel(x: column, y: row) {
                        pixels[y * outWidth + x] = value
                    }
                }
            }
            return true
        }
        return complete ? Raster.image(data, width: outWidth, height: outHeight) : nil
    }

    private static func fits(_ rect: CGRect) -> Bool {
        rect.width <= CGFloat(dimensionLimit) && rect.height <= CGFloat(dimensionLimit)
            && rect.width * rect.height <= CGFloat(pixelLimit)
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

    /// An image of `pixels`, in the layout the frames are read in.
    static func image(_ pixels: Data, width: Int, height: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: pixels as CFData) else { return nil }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                       space: colorSpace, bitmapInfo: CGBitmapInfo(rawValue: bitmapInfo), provider: provider, decode: nil,
                       shouldInterpolate: false, intent: .defaultIntent)
    }
}
