import CoreGraphics
import Foundation
import Vision

/// Positions in the stitched image use the same top-left pixel coordinates as a capture crop.
///
/// The image is checked against every frame it is made from, as the frames arrive. For each place of
/// the page the stitcher keeps the pixel the frames showed there, and each new frame is compared with
/// it pixel by pixel before any of it is kept. Two frames may show a place differently only where one
/// of them shows a fixed bar (a sticky header, a bottom bar, a side bar, a scroll bar), and a bar is a
/// band along an edge of the view; so the bands are whatever the disagreements themselves call for,
/// and nothing else decides them. A frame that disagrees where no such bands can account for it is
/// not stitched (`unmatched`), and what was stitched before it stays as it was. So in the image
/// 1. every pixel between the bars is the pixel of every stitched frame that showed that place between
///    its bars, and
/// 2. the bars are the first rows, last rows, first columns and last columns of the frames farthest
///    out that way: a header once at the top, a bottom bar once at the bottom.
/// A place that only ever lay under a bar, with page captured beyond it, has no pixel to show. It
/// stays transparent, as a place never visited; a manual pan can leave one. A frame whose bars would
/// leave such places in a page that had none is not stitched.
///
/// Bars are fixed things, and the page is what moves. Should two frames in a row show the page's own
/// detail in a bar, at places where what was kept as the page is flat, then something fixed and flat
/// was taken for the page and the page for bars (`keep`): nothing more is stitched and no image is made.
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
    /// content between them moves: a sticky header, a bottom bar, a frozen column, a side bar. These
    /// are found from two frames alone and are used only to register one against the other.
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

    static let pixelLimit = 48_000_000
    static let dimensionLimit = 32_000
    private var last: Raster?
    private var position = Offset.zero
    /// The rows of the page captured so far, from stitched row `firstRow` down.
    private var rows: [Row] = []
    private var firstRow = 0
    /// Where each stitched frame lies; a cell's `frame` counts into this from 1.
    private var origins = [Offset.zero]
    /// The bars of the view: every disagreement between two stitched frames lies in one of them.
    private var bands = Bands()
    /// The fixed bands the last registration found.
    private var bars: Fixed?
    /// Set when what was kept as the page turns out not to be the page (`keep`). No image is made.
    private var broken = false
    private(set) var bounds = CGRect.null

    /// The share of the view's width and height that moves with the content between the fixed bars;
    /// nil before the first move.
    var movingShare: CGSize? {
        guard let bars, let last else { return nil }
        let across = last.width - max(bars.left, bands.left) - max(bars.right, bands.right)
        let along = last.height - max(bars.top, bands.top) - max(bars.bottom, bands.bottom)
        return CGSize(width: Double(max(0, across)) / Double(last.width), height: Double(max(0, along)) / Double(last.height))
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
        guard !broken else { return .unmatched }
        guard let last else {
            keep(next, at: .zero)
            return .initial
        }
        guard next.width == last.width, next.height == last.height else { return .unmatched }
        if next.pixels == last.pixels { return .unchanged }
        guard let match = Self.registration(from: last, to: next) else { return .unmatched }
        let offset = match.offset
        guard offset != .zero else { return .unchanged }
        let origin = Offset(x: position.x + offset.x, y: position.y + offset.y)
        guard Self.fits(bounds.union(CGRect(x: origin.x, y: origin.y, width: next.width, height: next.height))) else { return .limit }
        // The check. Nothing of the frame is kept unless bars can account for every pixel in which it
        // differs from the frames before it, and the page between its bars reaches the page between
        // the bars of the frame before it.
        // Lines that stayed put beside the move, as the registration found them: a bar that way.
        let stayed = offset.y == 0 ? match.fixed.top + match.fixed.bottom > 0 : match.fixed.left + match.fixed.right > 0
        guard let agreed = disagreements(of: next, at: origin, moved: offset).bands(over: bands, stayedBeside: stayed),
              abs(offset.x) <= next.width - agreed.left - agreed.right,
              abs(offset.y) <= next.height - agreed.top - agreed.bottom else { return .unmatched }
        if agreed == bands {
            keep(next, at: origin)
        } else if Self.turned(from: last, to: next, moved: offset, bands: agreed) {
            return .unmatched
        } else {
            // Larger bars take the places under them out of the page. A frame whose bars would leave a
            // place with nothing to show inside a page that was whole before it is not stitched.
            var grown = self
            grown.bands = agreed
            grown.keep(next, at: origin)
            guard grown.broken || grown.whole || !whole else { return .unmatched }
            self = grown
        }
        guard !broken else { return .unmatched }
        bars = match.fixed
        return .moved(dx: offset.x, dy: offset.y)
    }

    /// The stitched image, each captured place once. Areas never visited in a manual pan stay
    /// transparent, and so do places no frame showed clear of a bar (`shows`). With `previewLimit`, a
    /// small picture of everything captured so far, no side longer than that.
    func image(previewLimit: Int? = nil) -> CGImage? {
        guard !bounds.isNull, !broken, let last else { return nil }
        let (minX, minY, width, height) = (Int(bounds.minX), Int(bounds.minY), Int(bounds.width), Int(bounds.height))
        if let previewLimit {
            // Three samples each way for every pixel of the picture, then scaled down smoothly.
            let scale = min(1, Double(previewLimit) / Double(max(width, height)))
            let small = (width: max(1, Int((Double(width) * scale).rounded(.up))), height: max(1, Int((Double(height) * scale).rounded(.up))))
            let fine = (width: min(width, small.width * 3), height: min(height, small.height * 3))
            var samples = [UInt32](repeating: 0, count: fine.width * fine.height)
            for y in 0..<fine.height {
                let row = rows[minY - firstRow + (y * 2 + 1) * height / (fine.height * 2)]
                for x in 0..<fine.width {
                    let column = minX + (x * 2 + 1) * width / (fine.width * 2) - row.x
                    if column >= 0, column < row.cells.count, row.cells[column].frame != 0 {
                        samples[y * fine.width + x] = row.cells[column].value
                    }
                }
            }
            guard let sampled = Raster.image(samples.withUnsafeBytes { Data($0) }, width: fine.width, height: fine.height) else { return nil }
            if fine.width == small.width, fine.height == small.height { return sampled }
            guard let context = CGContext(data: nil, width: small.width, height: small.height, bitsPerComponent: 8,
                                          bytesPerRow: 0, space: Raster.colorSpace, bitmapInfo: Raster.bitmapInfo) else { return nil }
            context.interpolationQuality = .medium
            context.setBlendMode(.copy)
            context.draw(sampled, in: CGRect(x: 0, y: 0, width: small.width, height: small.height))
            return context.makeImage()
        }
        let page = page
        var data = Data(count: width * height * 4)
        data.withUnsafeMutableBytes { bytes in
            let pixels = bytes.bindMemory(to: UInt32.self)
            for (index, row) in rows.enumerated() {
                let y = firstRow + index - minY
                row.cells.withUnsafeBufferPointer { cells in
                    for (column, cell) in cells.enumerated() where cell.frame != 0 {
                        if bands.clear(cell, last.width, last.height) || shows(cell, x: row.x + column, index: index, page) {
                            pixels[y * width + row.x + column - minX] = cell.value
                        }
                    }
                }
            }
        }
        return Raster.image(data, width: width, height: height)
    }

    /// For each row and each column of the capture, the first and the last place that a frame showed
    /// between its bars: where the page was seen.
    private struct Page {
        var rowFirst: [Int]
        var rowLast: [Int]
        var columnFirst: [Int]
        var columnLast: [Int]
    }

    private var page: Page {
        var page = Page(rowFirst: Array(repeating: .max, count: rows.count), rowLast: Array(repeating: .min, count: rows.count),
                        columnFirst: Array(repeating: .max, count: Int(bounds.width)),
                        columnLast: Array(repeating: .min, count: Int(bounds.width)))
        guard let last else { return page }
        let minX = Int(bounds.minX)
        for (index, row) in rows.enumerated() {
            row.cells.withUnsafeBufferPointer { cells in
                for (column, cell) in cells.enumerated() where cell.frame != 0 && bands.clear(cell, last.width, last.height) {
                    let x = row.x + column
                    page.rowFirst[index] = min(page.rowFirst[index], x)
                    page.rowLast[index] = x
                    page.columnFirst[x - minX] = min(page.columnFirst[x - minX], index)
                    page.columnLast[x - minX] = index
                }
            }
        }
        return page
    }

    /// Whether a place that no frame showed between its bars has a pixel to show: it lies in a bar of
    /// the frame its pixel is from, and no page was captured beyond it on the side of that bar, as
    /// above the header over the first content and below the bottom bar under the last. Otherwise it
    /// is a strip of bar inside the page, and the page under it was never seen.
    private func shows(_ cell: Cell, x: Int, index: Int, _ page: Page) -> Bool {
        guard let last else { return false }
        let from = origins[Int(cell.frame)]
        let (viewX, viewY, column) = (x - from.x, firstRow + index - from.y, x - Int(bounds.minX))
        return (viewY < bands.top && index < page.columnFirst[column])
            || (viewY >= last.height - bands.bottom && index > page.columnLast[column])
            || (viewX < bands.left && x < page.rowFirst[index])
            || (viewX >= last.width - bands.right && x > page.rowLast[index])
    }

    /// Whether every captured place has a pixel to show.
    private var whole: Bool {
        guard let last else { return false }
        let page = page
        for (index, row) in rows.enumerated() {
            for (column, cell) in row.cells.enumerated() where cell.frame != 0 && !bands.clear(cell, last.width, last.height) {
                if !shows(cell, x: row.x + column, index: index, page) { return false }
            }
        }
        return true
    }

    /// Every pixel in which `raster`, as a frame at `origin` that moved there by `moved`, differs
    /// between the bars from what earlier frames showed at the same place between their bars.
    private func disagreements(of raster: Raster, at origin: Offset, moved: Offset) -> Disagreements {
        let (width, height) = (raster.width, raster.height)
        var found = Disagreements(width: width, height: height, moved: (moved.x, moved.y))
        let bands = bands
        raster.pixels.withUnsafeBufferPointer { pixels in
            for y in bands.top..<(height - bands.bottom) {
                let index = origin.y + y - firstRow
                guard index >= 0, index < rows.count else { continue }
                let row = rows[index]
                let from = max(bands.left, row.x - origin.x)
                let to = min(width - bands.right, row.x + row.cells.count - origin.x)
                guard from < to else { continue }
                row.cells.withUnsafeBufferPointer { cells in
                    var column = origin.x + from - row.x
                    var pixel = y * width + from
                    for x in from..<to {
                        if cells[column].value != pixels[pixel], cells[column].frame != 0, bands.clear(cells[column], width, height) {
                            found.add(x: x, y: y, cells[column])
                        }
                        column += 1
                        pixel += 1
                    }
                }
                if found.unanswerable { break }
            }
        }
        return found
    }

    /// Whether larger bars `bands`, which account for every difference between a frame and the page
    /// kept so far, have the page and a bar the wrong way round. Bars are fixed things and the page is
    /// what moves. Here, for one of the bars, two things are true at once. The page's own detail
    /// moves through it: many of its pixels changed where they are, stand out sharply from their
    /// neighbors, and are the pixels the frame before showed one move away, also in that bar. And
    /// where that bar accounts for a difference between the two frames, the pixel it covers is such
    /// detail, new where it is, while the pixel kept as the page is flat. Then the flat thing that
    /// stays put is what is fixed (a panel in the middle of the view, say), the bar is page, and no
    /// bars along the edges account for the frame.
    private static func turned(from last: Raster, to raster: Raster, moved: Offset, bands: Bands) -> Bool {
        let (width, height) = (raster.width, raster.height)
        let (next, before) = (raster.pixels, last.pixels)
        /// The bar a view position lies in (0 top, 1 bottom, 2 left, 3 right), or nil.
        func bar(_ x: Int, _ y: Int) -> Int? {
            y < bands.top ? 0 : y >= height - bands.bottom ? 1 : x < bands.left ? 2 : x >= width - bands.right ? 3 : nil
        }
        /// Clearly different from the pixels on both sides of it, across or down.
        func sharp(_ pixels: [UInt32], _ x: Int, _ y: Int) -> Bool {
            let pixel = pixels[y * width + x]
            return (distance(pixels[y * width + x - 1], pixel) > 32 && distance(pixels[y * width + x + 1], pixel) > 32)
                || (distance(pixels[(y - 1) * width + x], pixel) > 32 && distance(pixels[(y + 1) * width + x], pixel) > 32)
        }
        func flat(_ pixels: [UInt32], _ x: Int, _ y: Int) -> Bool {
            let pixel = pixels[y * width + x]
            return pixels[y * width + x - 1] == pixel && pixels[y * width + x + 1] == pixel
                && pixels[(y - 1) * width + x] == pixel && pixels[(y + 1) * width + x] == pixel
        }
        var moving = [0, 0, 0, 0]
        var wrongWay = [0, 0, 0, 0]
        for y in 1..<(height - 1) {
            // Where the frame before showed the same place of the page.
            let lastY = y + moved.y
            guard lastY >= 1, lastY < height - 1 else { continue }
            for x in max(1, 1 - moved.x)..<min(width - 1, width - 1 - moved.x) {
                let lastX = x + moved.x
                let (pixel, earlier) = (next[y * width + x], before[lastY * width + lastX])
                switch (bar(x, y), bar(lastX, lastY)) {
                case let (here?, there):
                    // Detail that is new at this place in the view.
                    guard before[y * width + x] != pixel, sharp(next, x, y), !sharp(before, x, y) else { continue }
                    if there == here, earlier == pixel { moving[here] += 1 }
                    if there == nil, earlier != pixel, flat(before, lastX, lastY) { wrongWay[here] += 1 }
                case let (nil, there?):
                    if earlier != pixel, flat(next, x, y), next[lastY * width + lastX] != earlier,
                       sharp(before, lastX, lastY), !sharp(next, lastX, lastY) { wrongWay[there] += 1 }
                default:
                    break
                }
            }
        }
        return (0..<4).contains { moving[$0] >= 64 && wrongWay[$0] >= 64 }
    }

    /// Keep a frame that agrees with the frames before it. Between its bars its pixels are the page:
    /// they are kept where no frame showed the page yet, and elsewhere they are the pixels already
    /// there. In its bars a pixel is kept only where no frame showed the page, and there the frame
    /// farthest out that way wins: a frame that reaches past everything captured so far on the side
    /// of a bar replaces the bars of earlier frames, so a bottom bar ends up once, below the last
    /// content; any other bar stays behind them, so a header stays once above the first content, and
    /// scrolling back and forth near an end leaves its bar whole. Beside a tool bar, a frame that
    /// shows a place in fewer bars replaces one that showed it in more.
    ///
    /// A bar may show a place differently from the page kept there; that is what makes it a bar. But
    /// a pixel in a bar that changed where it is, is the pixel the frame before showed for the same
    /// place of the page, and stands out sharply from the pixels on both sides of it is the page's own
    /// detail moving through that bar. Where what is kept as the page at that place is flat, the kept
    /// page is not the page: it is something flat that stays put, and the capture is `broken`.
    private mutating func keep(_ raster: Raster, at origin: Offset) {
        let (width, height) = (raster.width, raster.height)
        let tile = CGRect(x: origin.x, y: origin.y, width: width, height: height)
        let leads = [tile.minY < bounds.minY, tile.maxY > bounds.maxY, tile.minX < bounds.minX, tile.maxX > bounds.maxX]
            .map { !bounds.isNull && $0 }
        reserve(origin, width, height)
        origins.append(origin)
        let (origins, bands, number) = (origins, bands, UInt32(origins.count - 1))
        let (middleX, middleY) = (width / 2, height / 2)
        let before = last?.pixels ?? []
        let (fromX, fromY) = (origin.x - position.x, origin.y - position.y)
        var (disagreed, misplaced) = (false, 0)
        raster.pixels.withUnsafeBufferPointer { pixels in
            for y in 0..<height {
                let index = origin.y + y - firstRow
                let rowZone = y < bands.top ? 0 : y >= height - bands.bottom ? 2 : 1
                let offset = origin.x - rows[index].x
                rows[index].cells.withUnsafeMutableBufferPointer { cells in
                    for x in 0..<width {
                        let columnZone = x < bands.left ? 0 : x >= width - bands.right ? 2 : 1
                        let old = cells[offset + x]
                        let pixel = pixels[y * width + x]
                        let seen = Cell(value: pixel, frame: number,
                                        above: y <= middleY ? Int16(y) : -1, below: y <= middleY ? .max : Int16(y),
                                        before: x <= middleX ? Int16(x) : -1, after: x <= middleX ? .max : Int16(x))
                        if old.frame == 0 {
                            cells[offset + x] = seen
                        } else if rowZone == 1, columnZone == 1 {
                            if !bands.clear(old, width, height) {
                                cells[offset + x] = seen
                            } else if old.value == pixel {
                                cells[offset + x].above = max(old.above, seen.above)
                                cells[offset + x].below = min(old.below, seen.below)
                                cells[offset + x].before = max(old.before, seen.before)
                                cells[offset + x].after = min(old.after, seen.after)
                            } else {
                                disagreed = true
                            }
                        } else if !bands.clear(old, width, height) {
                            // Two frames show this place in their bars.
                            var wins = leads[rowZone != 1 ? rowZone / 2 : 2 + columnZone / 2]
                            if !wins {
                                let was = origins[Int(old.frame)]
                                let (oldX, oldY) = (origin.x + x - was.x, origin.y + y - was.y)
                                let oldRowZone = oldY < bands.top ? 0 : oldY >= height - bands.bottom ? 2 : 1
                                let oldColumnZone = oldX < bands.left ? 0 : oldX >= width - bands.right ? 2 : 1
                                // In no bar the earlier frame did not show it in, at least as far out on
                                // the side of each, and in fewer bars or farther out on one.
                                if rowZone == 1 || rowZone == oldRowZone, columnZone == 1 || columnZone == oldColumnZone {
                                    let down = rowZone == 0 ? was.y - origin.y : rowZone == 2 ? origin.y - was.y : 0
                                    let across = columnZone == 0 ? was.x - origin.x : columnZone == 2 ? origin.x - was.x : 0
                                    let fewer = (rowZone == 1 && oldRowZone != 1) || (columnZone == 1 && oldColumnZone != 1)
                                    wins = down >= 0 && across >= 0 && (fewer || down > 0 || across > 0)
                                }
                            }
                            if wins { cells[offset + x] = seen }
                        } else if old.value != pixel, x > 0, x + 1 < width, offset + x > 0, offset + x + 1 < cells.count,
                                  y + fromY >= 0, y + fromY < height, x + fromX >= 0, x + fromX < width, !before.isEmpty,
                                  before[y * width + x] != pixel, before[(y + fromY) * width + x + fromX] == pixel,
                                  Self.distance(pixels[y * width + x - 1], pixel) > 32, Self.distance(pixels[y * width + x + 1], pixel) > 32 {
                            // The page's own detail in a bar, over a kept page that is flat there.
                            let (left, right) = (cells[offset + x - 1], cells[offset + x + 1])
                            if left.value == old.value, right.value == old.value, left.frame != 0, right.frame != 0,
                               bands.clear(left, width, height), bands.clear(right, width, height) { misplaced += 1 }
                        }
                    }
                }
            }
        }
        if disagreed || misplaced >= 32 { broken = true }
        bounds = bounds.union(tile)
        last = raster
        position = origin
    }

    /// Make room for a frame `width` × `height` at `origin`. A row that has to grow sideways grows by
    /// half as much again, so a long sideways capture does not copy every row at every step.
    private mutating func reserve(_ origin: Offset, _ width: Int, _ height: Int) {
        if rows.isEmpty { firstRow = origin.y }
        if origin.y < firstRow {
            rows.insert(contentsOf: repeatElement(Row(), count: firstRow - origin.y), at: 0)
            firstRow = origin.y
        }
        let end = origin.y + height - firstRow
        if end > rows.count { rows.append(contentsOf: repeatElement(Row(), count: end - rows.count)) }
        for index in (origin.y - firstRow)..<end {
            let row = rows[index]
            if row.cells.isEmpty {
                rows[index] = Row(x: origin.x, cells: ContiguousArray(repeating: Cell(), count: width))
                continue
            }
            let (from, to) = (row.x, row.x + row.cells.count)
            guard origin.x < from || origin.x + width > to else { continue }
            let spare = (to - from) / 2
            let start = origin.x < from ? min(origin.x, from - spare) : from
            let stop = origin.x + width > to ? max(origin.x + width, to + spare) : to
            var cells = ContiguousArray(repeating: Cell(), count: stop - start)
            cells.replaceSubrange((from - start)..<(to - start), with: row.cells)
            rows[index] = Row(x: start, cells: cells)
        }
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

/// What the frames showed at one place of the page.
private struct Cell {
    var value: UInt32 = 0
    /// The frame the pixel is from, counted from 1; 0 where nothing was captured.
    var frame: UInt32 = 0
    /// Of the view rows at which frames showed this pixel here, the one nearest the middle row from
    /// above (-1 when none) and from below (`Int16.max` when none), and the same for the view columns
    /// from the left and from the right. No bar reaches the middle of the view, so the place was shown
    /// between the bars exactly when one of each pair lies between them (`Bands.clear`).
    var above: Int16 = -1
    var below: Int16 = .max
    var before: Int16 = -1
    var after: Int16 = .max
}

/// One row of the page: the cells from stitched column `x` on.
private struct Row {
    var x = 0
    var cells: ContiguousArray<Cell> = []
}

/// The bars of the view: so many rows at its top and bottom and columns at its left and right. They
/// only ever grow.
private struct Bands: Equatable {
    var top = 0
    var bottom = 0
    var left = 0
    var right = 0

    /// Whether a frame showed the cell's pixel at its place clear of every bar. With bars along both
    /// axes and a place shown at several rows and columns this can say yes where each showing lay in
    /// one bar; it never says no for a place a frame showed clear of them.
    func clear(_ cell: Cell, _ width: Int, _ height: Int) -> Bool {
        (Int(cell.above) >= top || Int(cell.below) < height - bottom) && (Int(cell.before) >= left || Int(cell.after) < width - right)
    }
}

/// The pixels in which a new frame differs, between the bars, from what earlier frames showed at the
/// same places between their bars, and the larger bars that would account for all of them.
///
/// One of the two frames shows a bar at such a place, and a band over the place in the view where it
/// showed it accounts for the difference: a band over the new frame's line, or over every line at which
/// earlier frames showed their pixel. Along each axis that is a band at the start, one at the end,
/// either or neither. The bars chosen are the thinnest that account for every difference.
private struct Disagreements {
    /// A band beside the direction of the move takes its strip of the page out of the check for the
    /// whole length of the capture; a band across the move only the lines at one end of it. So where
    /// either would do, the band across the move is taken unless it is more than twice as thick. But
    /// where the two frames themselves show lines that stayed put beside the move (the registration
    /// found them), a bar lies that way, and a band beside the move is taken when it is the thinner.
    private static let besideTheMove = 2
    private static let never = Int32.max
    let width: Int
    let height: Int
    /// The bars across the move are the left and right ones: the frame moved sideways only.
    private let sideways: Bool
    /// The frame moved along one axis only.
    private let straight: Bool
    /// The thickest band across the move, and beside it: a band stops short of the middle line.
    private let reach: (across: Int, beside: Int)
    private var count = 0
    /// For each pair of bands across the move, one at the start and one at the end, either of which
    /// would account for a difference (index: start × (reach + 2) + end, `reach + 1` where that band
    /// cannot): the thickest bands beside the move, at the start and at the end, that the differences
    /// of that pair would need instead; `never` where no band beside the move can account for one.
    private var needs: [(start: Int32, end: Int32)] = []
    private var asked = (start: [Bool](), end: [Bool]())
    /// A difference lies where no band can reach, from either frame.
    private(set) var unanswerable = false

    init(width: Int, height: Int, moved: (x: Int, y: Int)) {
        self.width = width
        self.height = height
        sideways = moved.y == 0
        straight = moved.x == 0 || moved.y == 0
        reach = sideways ? ((width - 1) / 2, (height - 1) / 2) : ((height - 1) / 2, (width - 1) / 2)
    }

    /// The frame shows a pixel at view (`x`, `y`) that is not the pixel of `cell`.
    mutating func add(x: Int, y: Int, _ cell: Cell) {
        if count == 0 {
            needs = Array(repeating: (0, 0), count: (reach.across + 2) * (reach.across + 2))
            asked = (Array(repeating: false, count: reach.across + 2), Array(repeating: false, count: reach.across + 2))
        }
        count += 1
        let rows = Self.bands(at: y, of: height, nearStart: Int(cell.above), nearEnd: Int(cell.below))
        let columns = Self.bands(at: x, of: width, nearStart: Int(cell.before), nearEnd: Int(cell.after))
        let (across, beside) = sideways ? (columns, rows) : (rows, columns)
        var need = (start: Self.never, end: Self.never)
        if beside.start <= reach.beside || beside.end <= reach.beside {
            need = beside.start <= beside.end ? (Int32(beside.start), 0) : (0, Int32(beside.end))
        } else if across.start > reach.across, across.end > reach.across {
            unanswerable = true
            return
        }
        let index = across.start * (reach.across + 2) + across.end
        needs[index] = (max(needs[index].start, need.start), max(needs[index].end, need.end))
        asked.start[across.start] = true
        asked.end[across.end] = true
    }

    /// The thinnest bands along one axis of a view `length` lines long, one at its start and one at
    /// its end, each of which accounts for a difference: over the frame's own line `line`, or over
    /// every line at which earlier frames showed their pixel, which one band can cover only when all
    /// of them lie on one side of the middle. `(length - 1) / 2 + 1` for a band that cannot: it would
    /// reach the middle line.
    private static func bands(at line: Int, of length: Int, nearStart: Int, nearEnd: Int) -> (start: Int, end: Int) {
        let reach = (length - 1) / 2
        var (start, end) = (reach + 1, reach + 1)
        if line <= length / 2 { start = min(start, line + 1) } else { end = min(end, length - line) }
        if nearEnd == Int(Int16.max), nearStart >= 0 { start = min(start, nearStart + 1) }
        if nearStart < 0, nearEnd != Int(Int16.max) { end = min(end, length - nearEnd) }
        return (min(start, reach + 1), min(end, reach + 1))
    }

    /// The thinnest bars, no smaller than `current`, that account for every difference; nil when no
    /// bars can, or when they would leave less than a quarter of the view between them.
    func bands(over current: Bands, stayedBeside: Bool) -> Bands? {
        guard !unanswerable else { return nil }
        guard count > 0 else { return current }
        // What the differences that bands of s and e lines across the move do not account for need
        // beside it: those of every pair that asks for more than s lines and more than e lines.
        let side = reach.across + 2
        var beyond = needs
        for start in (0..<side).reversed() {
            for end in (0..<side).reversed() {
                var most = beyond[start * side + end]
                if start + 1 < side {
                    let next = beyond[(start + 1) * side + end]
                    most = (max(most.start, next.start), max(most.end, next.end))
                }
                if end + 1 < side {
                    let next = beyond[start * side + end + 1]
                    most = (max(most.start, next.start), max(most.end, next.end))
                }
                beyond[start * side + end] = most
            }
        }
        let now = sideways ? (across: (current.left, current.right), beside: (current.top, current.bottom))
            : (across: (current.top, current.bottom), beside: (current.left, current.right))
        let (along, other) = sideways ? (width, height) : (height, width)
        let starts = [now.across.0] + (0...reach.across).filter { asked.start[$0] && $0 > now.across.0 }
        let ends = [now.across.1] + (0...reach.across).filter { asked.end[$0] && $0 > now.across.1 }
        let weight = straight && !stayedBeside ? Self.besideTheMove : 1
        var best: (across: (Int, Int), beside: (Int, Int), cost: Int)?
        for start in starts {
            for end in ends where along - start - end >= max(8, along / 4) {
                let need = beyond[(start + 1) * side + end + 1]
                guard need.start != Self.never, need.end != Self.never else { continue }
                let beside = (max(now.beside.0, Int(need.start)), max(now.beside.1, Int(need.end)))
                guard beside.0 <= reach.beside, beside.1 <= reach.beside, other - beside.0 - beside.1 >= max(8, other / 4) else { continue }
                let cost = start + end + (beside.0 + beside.1) * weight
                // At the same cost, the thinner bands beside the move.
                if best.map({ cost < $0.cost || (cost == $0.cost && beside.0 + beside.1 < $0.beside.0 + $0.beside.1) }) ?? true {
                    best = ((start, end), beside, cost)
                }
            }
        }
        guard let best else { return nil }
        return sideways ? Bands(top: best.beside.0, bottom: best.beside.1, left: best.across.0, right: best.across.1)
            : Bands(top: best.across.0, bottom: best.across.1, left: best.beside.0, right: best.beside.1)
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
