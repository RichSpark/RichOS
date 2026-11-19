import CoreGraphics
import CoreText
import XCTest
@testable import RichGlance

final class ScrollStitcherTests: XCTestCase {
    func testVerticalCaptureDoesNotRepeatOverlap() throws {
        let document = fixture(width: 120, height: 280)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 120, h: 140)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 57, w: 120, h: 140)), .moved(dx: 0, dy: 57))
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 140, w: 120, h: 140)), .moved(dx: 0, dy: 83))
        try check(sut, against: document)
    }

    func testHorizontalCaptureCanScrollBackPastTheStart() throws {
        let document = fixture(width: 257, height: 120)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 70, y: 0, w: 130, h: 120)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 127, y: 0, w: 130, h: 120)), .moved(dx: 57, dy: 0))
        XCTAssertEqual(sut.append(crop(document, x: 70, y: 0, w: 130, h: 120)), .moved(dx: -57, dy: 0))
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 130, h: 120)), .moved(dx: -70, dy: 0))
        try check(sut, against: document)
    }

    func testBothDirectionsCoverTheFullGridAfterAlternatingSweeps() throws {
        let document = fixture(width: 185, height: 176)
        var sut = ScrollStitcher()
        for (x, y) in [(0, 0), (65, 0), (65, 56), (0, 56)] {
            _ = sut.append(crop(document, x: x, y: y, w: 120, h: 120))
        }
        try check(sut, against: document)
    }

    func testDiagonalPanLeavesUnvisitedCornersTransparent() throws {
        let document = fixture(width: 174, height: 166)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 120, h: 120)), .initial)
        XCTAssertEqual(sut.append(crop(document, x: 54, y: 46, w: 120, h: 120)), .moved(dx: 54, dy: 46))
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, 174)
        XCTAssertEqual(actual.height, 166)
        XCTAssertEqual(pixels(crop(actual, x: 0, y: 0, w: 120, h: 120)),
                       pixels(crop(document, x: 0, y: 0, w: 120, h: 120)))
        XCTAssertEqual(pixels(crop(actual, x: 54, y: 46, w: 120, h: 120)),
                       pixels(crop(document, x: 54, y: 46, w: 120, h: 120)))
        XCTAssertTrue(pixels(crop(actual, x: 120, y: 0, w: 54, h: 46)).allSatisfy { $0 == 0 })
    }

    func testShortEdgeStepOnWideTableIsStillRegistered() throws {
        let width = 1091
        let height = 362
        var values: [UInt32] = []
        for y in 0..<height {
            let row = y / 31
            for x in 0..<width {
                let column = x / 137
                if y % 31 == 30 || x % 137 == 136 {
                    values.append(0xffff_ffff)
                } else {
                    let r = UInt32((row * 17 + column * 31 + 157) % 125 + 120)
                    let g = UInt32((row * 53 + column * 11 + 83) % 125 + 120)
                    let b = UInt32((row * 71 + column * 91 + 117) % 125 + 120)
                    values.append(0xff00_0000 | r << 16 | g << 8 | b)
                }
            }
        }
        let document = image(width: width, height: height, pixels: values)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 0, w: 576, h: height)), .initial)
        for x in [162, 324, 486] {
            XCTAssertEqual(sut.append(crop(document, x: x, y: 0, w: 576, h: height)), .moved(dx: 162, dy: 0))
        }
        XCTAssertEqual(sut.append(crop(document, x: 515, y: 0, w: 576, h: height)), .moved(dx: 29, dy: 0))
        try check(sut, against: document)
    }

    func testDuplicateAndUnrelatedFramesCannotCreateFalseSeams() throws {
        let document = fixture(width: 156, height: 220)
        let first = crop(document, x: 0, y: 0, w: 156, h: 120)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(first), .initial)
        XCTAssertEqual(sut.append(first), .unchanged)
        XCTAssertEqual(sut.append(fixture(width: 156, height: 120, salt: 111)), .unmatched)
        XCTAssertEqual(sut.append(crop(document, x: 0, y: 61, w: 156, h: 120)), .moved(dx: 0, dy: 61))
        try check(sut, against: crop(document, x: 0, y: 0, w: 156, h: 181))
    }

    func testFixedHeaderDoesNotStopAutomaticScrollAndAppearsOnce() throws {
        // A sticky site header stays put while the text under it scrolls. Steps are 28% of
        // the viewport, as automatic scrolling sends them.
        let page = textPage(width: 800, height: 2000)
        let viewport = 500
        let step = 140
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: 0, w: 800, h: viewport), rows: 64)), .initial)
        var y = 0
        while y + viewport < page.height {
            let next = min(page.height - viewport, y + step)
            XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: next, w: 800, h: viewport), rows: 64)),
                           .moved(dx: 0, dy: next - y), "step to y=\(next)")
            y = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, 800)
        XCTAssertEqual(actual.height, page.height)
        // The first view, header included, then the page below it without another header.
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: viewport))
                      == pixels(withHeader(crop(page, x: 0, y: 0, w: 800, h: viewport), rows: 64)))
        XCTAssertTrue(pixels(crop(actual, x: 0, y: viewport, w: 800, h: page.height - viewport))
                      == pixels(crop(page, x: 0, y: viewport, w: 800, h: page.height - viewport)))
    }

    func testTranslucentHeaderBesideAMovingScrollBarDoesNotStopAutomaticScroll() throws {
        // Content shows faintly through a 90% opaque header, and the page's scroll bar thumb moves
        // beside it, so no header row is identical from one view to the next.
        let page = textPage(width: 800, height: 2000)
        let viewport = 500
        func view(_ y: Int) -> CGImage {
            withScrollBar(withHeader(crop(page, x: 0, y: y, w: 800, h: viewport), rows: 64, opacity: 0.9),
                          at: y, of: page.height)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        var y = 0
        while y + viewport < page.height {
            let next = min(page.height - viewport, y + 140)
            XCTAssertEqual(sut.append(view(next)), .moved(dx: 0, dy: next - y), "step to y=\(next)")
            y = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, page.height)
        guard actual.height == page.height else { return }
        // The header appears once, in the first view.
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: viewport)) == pixels(view(0)))
    }

    func testEightyPercentOpaqueHeaderDoesNotStopAutomaticScroll() throws {
        // An ordinary translucent header lets a fifth of the text under it show through, and the
        // page's scroll bar moves beside it.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withScrollBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 500), rows: 64, opacity: 0.8),
                          at: y, of: page.height)
        }
        let sut = scrollDown(page.height, viewport: 500, step: 140, view: view)
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, page.height)
        guard actual.height == page.height else { return }
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: 500)) == pixels(view(0)), "the header appears once, at the top")
    }

    func testHeaderFillingMostOfTheTopHalfDoesNotStopAutomaticScroll() throws {
        // A multirow site header in a short selection: more than two fifths of the view stays put.
        let page = textPage(width: 800, height: 2000)
        for (viewport, header) in [(300, 128), (500, 224)] {
            func view(_ y: Int) -> CGImage { withHeader(crop(page, x: 0, y: y, w: 800, h: viewport), rows: header) }
            let sut = scrollDown(page.height, viewport: viewport, step: viewport * 28 / 100, view: view)
            let actual = try XCTUnwrap(sut.image())
            XCTAssertEqual(actual.height, page.height, "\(header)-pixel header in a \(viewport)-pixel view")
            guard actual.height == page.height else { continue }
            XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: viewport)) == pixels(view(0)),
                          "the \(header)-pixel header appears once, at the top")
            XCTAssertTrue(pixels(crop(actual, x: 0, y: viewport, w: 800, h: page.height - viewport))
                          == pixels(crop(page, x: 0, y: viewport, w: 800, h: page.height - viewport)),
                          "the page below the first view has no header in it")
        }
    }

    func testBottomBarIsCapturedToTheEndAndAppearsOnceAtTheBottom() throws {
        // A translucent sticky header and a bar fixed to the bottom of the view, as on many web pages.
        // Automatic steps, then manual ones: back up, shorter than the bar, and back and forth at the end.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBottomBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 500), rows: 64, opacity: 0.8), rows: 56)
        }
        let automatic = scrollDown(page.height, viewport: 500, step: 140, view: view)
        var manual = ScrollStitcher()
        XCTAssertEqual(manual.append(view(0)), .initial)
        var y = 0
        for next in [40, 230, 180, 400, 640, 630, 870, 1100, 1340, 1500, 1440, 1485] {
            XCTAssertEqual(manual.append(view(next)), .moved(dx: 0, dy: next - y), "manual step to y=\(next)")
            y = next
        }
        for (name, sut) in [("automatic", automatic), ("manual", manual)] {
            let actual = try XCTUnwrap(sut.image())
            XCTAssertEqual(actual.height, page.height, name)
            guard actual.height == page.height else { continue }
            XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: 64)) == pixels(crop(view(0), x: 0, y: 0, w: 800, h: 64)),
                          "\(name): the header appears once, at the top")
            XCTAssertTrue(pixels(crop(actual, x: 0, y: 64, w: 800, h: page.height - 120))
                          == pixels(crop(page, x: 0, y: 64, w: 800, h: page.height - 120)),
                          "\(name): the whole page between them, with no header or bar in it")
            XCTAssertTrue(pixels(crop(actual, x: 0, y: page.height - 56, w: 800, h: 56))
                          == pixels(crop(view(1500), x: 0, y: 444, w: 800, h: 56)),
                          "\(name): the bar appears once, at the bottom")
        }
    }

    func testRightBarIsCapturedToTheEndAndAppearsOnceAtTheRight() throws {
        // A bar fixed to the right of the view while a wide page scrolls to the right.
        let page = fixture(width: 2000, height: 400)
        func view(_ x: Int) -> CGImage { withRightBar(crop(page, x: x, y: 0, w: 600, h: 400), columns: 60) }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        var x = 0
        while x + 600 < page.width {
            let next = min(page.width - 600, x + 168)
            XCTAssertEqual(sut.append(view(next)), .moved(dx: next - x, dy: 0), "step to x=\(next)")
            x = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, page.width)
        guard actual.width == page.width else { return }
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: page.width - 60, h: 400))
                      == pixels(crop(page, x: 0, y: 0, w: page.width - 60, h: 400)), "the whole page, with no bar in it")
        XCTAssertTrue(pixels(crop(actual, x: page.width - 60, y: 0, w: 60, h: 400))
                      == pixels(crop(view(x), x: 540, y: 0, w: 60, h: 400)), "the bar appears once, at the right")
    }

    func testTallHeaderAndBottomBarInAShortSelectionAreCapturedToTheEnd() throws {
        // A 128-pixel sticky header and an 88-pixel cookie bar leave 84 of 300 rows moving: a 28% step
        // would scroll all of them away. Steps are sized as automatic scrolling sizes them.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBottomBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 300), rows: 128), rows: 88)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        var y = 0
        while y + 300 < page.height {
            let step = Int(ScrollStitcher.automaticStep(along: 300, moving: sut.movingShare.map { Double($0.height) }).rounded())
            let next = min(page.height - 300, y + step)
            let update = sut.append(view(next))
            XCTAssertEqual(update, .moved(dx: 0, dy: next - y), "step to y=\(next)")
            guard update == .moved(dx: 0, dy: next - y) else { break }
            y = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, page.height)
        guard actual.height == page.height else { return }
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: 128)) == pixels(crop(view(0), x: 0, y: 0, w: 800, h: 128)),
                      "the header appears once, at the top")
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 128, w: 800, h: page.height - 216))
                      == pixels(crop(page, x: 0, y: 128, w: 800, h: page.height - 216)), "the whole page between them")
        XCTAssertTrue(pixels(crop(actual, x: 0, y: page.height - 88, w: 800, h: 88))
                      == pixels(crop(view(y), x: 0, y: 212, w: 800, h: 88)), "the bar appears once, at the bottom")
    }

    func testWhiteBottomBarWithBlankMarginsLeavesEveryPagePixelInPlace() throws {
        // An ordinary white cookie bar: dark labels, blank margins, with or without a faint divider.
        // Its blank rows look like blank page, so they stayed in the image over the text under them.
        for width in [800, 1200] {
            let page = textPage(width: width, height: 2000)
            for divider in [false, true] {
                func view(_ y: Int) -> CGImage {
                    withWhiteBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 500), rows: 64, opacity: 0.8),
                                 rows: 56, divider: divider)
                }
                var sut = ScrollStitcher()
                XCTAssertEqual(sut.append(view(0)), .initial)
                var y = 0
                while y < 1500 {
                    let step = Int(ScrollStitcher.automaticStep(along: 500, moving: sut.movingShare.map { Double($0.height) }).rounded())
                    let next = min(1500, y + step)
                    let update = sut.append(view(next))
                    XCTAssertEqual(update, .moved(dx: 0, dy: next - y), "width \(width), divider \(divider), step to y=\(next)")
                    guard update == .moved(dx: 0, dy: next - y) else { break }
                    y = next
                }
                let actual = try XCTUnwrap(sut.image())
                XCTAssertEqual(actual.height, 2000)
                guard y == 1500, actual.height == 2000 else { continue }
                XCTAssertEqual(wrong(actual, page, x: 0, y: 64, w: 800, h: 1880), 0,
                               "wrong pixels between the header and the bar, width \(width), divider \(divider)")
                XCTAssertTrue(pixels(crop(actual, x: 0, y: 1944, w: 800, h: 56)) == pixels(crop(view(1500), x: 0, y: 444, w: 800, h: 56)),
                              "the bar appears once, at the bottom")
            }
        }
    }

    func testSidewaysThenDownwardCaptureLeavesNoStripOfABarInThePage() throws {
        // A header and a bottom bar whose labels sit between blank margins, scrolled right, down,
        // left, down and right. The margins stayed in the image as strips across the page.
        let page = textPage(width: 1000, height: 1000)
        func view(_ x: Int, _ y: Int) -> CGImage {
            withBottomBar(withHeader(crop(page, x: x, y: y, w: 600, h: 500), rows: 64), rows: 56)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0, 0)), .initial)
        var last = (x: 0, y: 0)
        for next in [(x: 168, y: 0), (x: 168, y: 140), (x: 0, y: 140), (x: 0, y: 280), (x: 168, y: 280)] {
            XCTAssertEqual(sut.append(view(next.x, next.y)), .moved(dx: next.x - last.x, dy: next.y - last.y), "move to \(next)")
            last = next
        }
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.width, 768)
        XCTAssertEqual(actual.height, 780)
        guard actual.width == 768, actual.height == 780 else { return }
        XCTAssertEqual(wrong(actual, page, x: 0, y: 64, w: 768, h: 660), 0, "wrong pixels between the header and the bar")
    }

    func testSweepsBesideARightToolBarLeaveNoStripOfABarInThePage() throws {
        // A see-through header, a white cookie bar and a tool bar down the right, captured in sideways
        // sweeps at five heights. Every pixel between the bars is seen clear of them in some frame,
        // yet strips of the tool bar stayed in the image, over blank page a later frame showed.
        let page = textPage(width: 1000, height: 1000)
        func view(_ at: (x: Int, y: Int)) -> CGImage {
            withRightBar(withWhiteBar(withHeader(crop(page, x: at.x, y: at.y, w: 600, h: 500), rows: 64, opacity: 0.8),
                                      rows: 56, divider: false), columns: 72)
        }
        let path = [(0, 0), (168, 0), (336, 0), (400, 0), (400, 140), (232, 140), (64, 140), (0, 140), (0, 280), (168, 280),
                    (336, 280), (400, 280), (400, 420), (232, 420), (64, 420), (0, 420), (0, 500), (168, 500), (336, 500),
                    (400, 500)].map { (x: $0.0, y: $0.1) }
        let actual = try XCTUnwrap(sweep(path, view: view))
        XCTAssertEqual(wrong(actual, page, x: 0, y: 64, w: 928, h: 880), 0, "wrong pixels between the bars")
    }

    func testSidewaysSweepsUpwardUnderASeeThroughHeaderLeaveNoneOfItInThePage() throws {
        // The same sweeps from the bottom of the page up. A sideways move cannot tell a see-through
        // header from the page moving under it; its rows stayed in the image from the first sweep.
        let page = textPage(width: 1000, height: 1000)
        func view(_ at: (x: Int, y: Int)) -> CGImage {
            withWhiteBar(withHeader(crop(page, x: at.x, y: at.y, w: 600, h: 500), rows: 64, opacity: 0.8), rows: 56, divider: false)
        }
        let path = [(400, 500), (336, 500), (168, 500), (0, 500), (0, 420), (64, 420), (232, 420), (400, 420), (400, 280),
                    (336, 280), (168, 280), (0, 280), (0, 140), (64, 140), (232, 140), (400, 140), (400, 0), (336, 0), (168, 0),
                    (0, 0)].map { (x: $0.0, y: $0.1) }
        let actual = try XCTUnwrap(sweep(path, view: view))
        XCTAssertEqual(wrong(actual, page, x: 0, y: 64, w: 1000, h: 880), 0, "wrong pixels between the header and the bar")
    }

    /// A capture through the views at `path`, each move of which must be stitched, of a 1000 × 1000 page.
    private func sweep(_ path: [(x: Int, y: Int)], view: ((x: Int, y: Int)) -> CGImage,
                       file: StaticString = #filePath, line: UInt = #line) -> CGImage? {
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(path[0])), .initial, file: file, line: line)
        for (last, next) in zip(path, path.dropFirst()) {
            let update = sut.append(view(next))
            XCTAssertEqual(update, .moved(dx: next.x - last.x, dy: next.y - last.y), "move to \(next)", file: file, line: line)
            guard update == .moved(dx: next.x - last.x, dy: next.y - last.y) else { return nil }
        }
        guard let actual = sut.image() else { XCTFail("no image", file: file, line: line); return nil }
        XCTAssertEqual(actual.width, 1000, file: file, line: line)
        XCTAssertEqual(actual.height, 1000, file: file, line: line)
        return actual.width == 1000 && actual.height == 1000 ? actual : nil
    }

    func testFixedHeaderOverAListOfShortNamesDoesNotStopAutomaticScroll() throws {
        // Short file names on a white page change only a few pixels in each row, yet they scroll.
        let width = 800
        let height = 2000
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        var seed: UInt32 = 7
        for row in 0..<(height / 24) {
            seed = seed &* 1_103_515_245 &+ 12_345
            let line = CTLineCreateWithAttributedString(NSAttributedString(string: String(format: "%08X.txt", seed), attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, 13, nil),
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0.1, alpha: 1)
            ]))
            context.textPosition = CGPoint(x: 20, y: height - 24 - row * 24)
            CTLineDraw(line, context)
        }
        let page = context.makeImage()!
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: 0, w: width, h: 500), rows: 64)), .initial)
        for y in stride(from: 140, through: height - 500, by: 140) {
            XCTAssertEqual(sut.append(withHeader(crop(page, x: 0, y: y, w: width, h: 500), rows: 64)),
                           .moved(dx: 0, dy: 140), "step to y=\(y)")
        }
        XCTAssertEqual(Int(sut.bounds.height), 500 + (height - 500) / 140 * 140)
    }

    func testRetinaTextStepIsRegisteredWhenSampleRowsMatchTheLinePitch() throws {
        // A 600 × 700 pt viewport at 2x over plain text. Evenly spaced sample rows matched the
        // line pitch here and landed between lines, so these 28% steps were refused.
        let page = textPage(width: 1200, height: 2600, scale: 2)
        for y in [168, 448, 476, 756] {
            var sut = ScrollStitcher()
            XCTAssertEqual(sut.append(crop(page, x: 0, y: y, w: 1200, h: 1400)), .initial)
            XCTAssertEqual(sut.append(crop(page, x: 0, y: y + 392, w: 1200, h: 1400)), .moved(dx: 0, dy: 392),
                           "step from y=\(y)")
        }
    }

    // MARK: - The image agrees with every frame it is made from, or the frame is not stitched

    func testCookieBarInsetFromTheSidesLeavesEveryPagePixelInPlace() throws {
        // A white cookie bar that stops 50 pixels short of each side of the view. The page moving beside
        // it made part of the bar count as page, and those rows of it stayed in the image over the text
        // under them, with every move stitched and nothing said.
        let page = textPage(width: 800, height: 2000)
        for header in [64, 0] {
            func view(_ y: Int) -> CGImage {
                let view = crop(page, x: 0, y: y, w: 800, h: 500)
                return withCookieBar(header > 0 ? withHeader(view, rows: header, opacity: 0.8) : view, rows: 56, inset: 50)
            }
            let (sut, reached) = automaticDown(to: 1500, viewport: 500, view: view)
            let actual = try exactOrNotStitched(sut, page: page, header: header, bar: 56, "inset bar under a \(header)-pixel header")
            XCTAssertEqual(reached, 1500, "inset bar under a \(header)-pixel header: captured to the end")
            XCTAssertEqual(actual?.height, 2000)
            guard let actual, actual.height == 2000 else { continue }
            XCTAssertTrue(pixels(crop(actual, x: 0, y: 1944, w: 800, h: 56)) == pixels(crop(view(1500), x: 0, y: 444, w: 800, h: 56)),
                          "the bar appears once, at the bottom, with the page beside it")
        }
    }

    func testSeeThroughBottomBarLeavesEveryPagePixelInPlace() throws {
        // A bottom bar across the whole view that lets a fifth, or two fifths, of the page show through.
        // The text moving under it made its rows count as page, and they stayed in the image.
        let page = textPage(width: 800, height: 2000)
        for opacity in [0.8, 0.6] {
            func view(_ y: Int) -> CGImage {
                withCookieBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 500), rows: 64, opacity: 0.8), rows: 56, inset: 0,
                              opacity: opacity)
            }
            let (sut, reached) = automaticDown(to: 1500, viewport: 500, view: view)
            let actual = try exactOrNotStitched(sut, page: page, header: 64, bar: 56, "bottom bar at \(opacity)")
            XCTAssertEqual(reached, 1500, "bottom bar at \(opacity): captured to the end")
            XCTAssertEqual(actual?.height, 2000)
        }
    }

    func testHeaderThatGrowsWhenScrollingBackLeavesNoneOfItInThePage() throws {
        // A sticky header that is 32 pixels tall while the page scrolls down and 64 once it scrolls
        // back up. The added rows stayed in the image as page, over text later frames showed clear.
        let page = textPage(width: 800, height: 2000)
        let path = [500, 520, 500, 480, 460, 320, 180, 40, 0]
        func view(_ index: Int) -> CGImage {
            withHeader(crop(page, x: 0, y: path[index], w: 800, h: 500), rows: index < 2 ? 32 : 64, opacity: 0.8)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        var stitched = 0
        for index in 1..<path.count {
            guard sut.append(view(index)) == .moved(dx: 0, dy: path[index] - path[index - 1]) else { break }
            stitched = index
        }
        // What is offered starts at the highest view stitched; under its header every pixel is the page's.
        let top = path[...stitched].min()!
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(wrong(actual, crop(page, x: 0, y: top, w: 800, h: actual.height), x: 0, y: 64, w: 800, h: actual.height - 64), 0,
                       "wrong pixels under the header")
        XCTAssertEqual(stitched, path.count - 1, "every move is stitched")
        XCTAssertEqual(actual.height, 1020)
        guard stitched == path.count - 1, actual.height == 1020 else { return }
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 0, w: 800, h: 64)) == pixels(crop(view(path.count - 1), x: 0, y: 0, w: 800, h: 64)),
                      "the header appears once, at the top, as the highest view showed it")
    }

    func testFloatingButtonNearACornerAppearsOnceAndLeavesThePageWhole() throws {
        // A chat button that floats over the page: 56 pixels square, 20 pixels in from the right and
        // from the bottom of the view.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBox(crop(page, x: 0, y: y, w: 800, h: 500), x: 724, y: 424, w: 56, h: 56)
        }
        let (sut, reached) = automaticDown(to: 1500, viewport: 500, view: view)
        let actual = try exactOrNotStitched(sut, page: page, header: 0, bar: 76, "floating button")
        XCTAssertEqual(reached, 1500, "captured to the end")
        XCTAssertEqual(actual?.height, 2000)
        guard let actual, actual.height == 2000 else { return }
        XCTAssertTrue(pixels(crop(actual, x: 0, y: 1924, w: 800, h: 76)) == pixels(crop(view(1500), x: 0, y: 424, w: 800, h: 76)),
                      "the button appears once, where the last view showed it")
    }

    func testSomethingFixedInTheMiddleOfTheViewIsNotStitchedIntoThePage() throws {
        // A panel that stays in the middle of the view while the page scrolls under it is no bar along
        // an edge. No image of the page can agree with both views, so the second is not stitched.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBox(crop(page, x: 0, y: y, w: 800, h: 500), x: 300, y: 200, w: 200, h: 100)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        XCTAssertEqual(sut.append(view(50)), .unmatched)
        XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(view(0)), "what is offered is the first view, as captured")
    }

    func testTallFlatPanelInTheMiddleIsNeverSmearedDownThePage() throws {
        // A flat panel 220 pixels tall that stays in the middle of the view. For one move it can pass
        // for a stripe of the page between two bars, with the page itself taken for the bars; but then
        // the text of the page shows in those bars where the stripe was kept, two views in a row, and
        // that is no bar. Whatever the capture offers after that is not a page with the panel smeared
        // down it: here, nothing.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage { withBox(crop(page, x: 0, y: y, w: 800, h: 500), x: 300, y: 140, w: 200, h: 220) }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        _ = sut.append(view(50))
        XCTAssertEqual(sut.append(view(160)), .unmatched)
        XCTAssertEqual(sut.append(view(270)), .unmatched)
        if let offered = sut.image() {
            XCTAssertTrue(pixels(offered) == pixels(view(0)), "only the first view, as captured, may be offered")
        }
    }

    func testFlatPanelOverAShortPageIsNotStretchedByTheOnlyMove() throws {
        // A page only 50 pixels longer than the view, under a flat panel that stays in the middle. The
        // one move there is was explained by a bar 140 pixels tall at the top and another at the
        // bottom, with the panel kept as a stripe of the page: the image offered at the end of the page
        // had the panel 50 pixels taller than it is, over text both views showed. The page's text moves
        // through those bars, and where they account for a difference it is the text they cover and
        // the flat panel they keep: the wrong way round. The move is not stitched.
        for scale in [1, 2] as [CGFloat] {
            let page = textPage(width: 800, height: 550, scale: scale)
            func view(_ y: Int) -> CGImage { withBox(crop(page, x: 0, y: y, w: 800, h: 500), x: 300, y: 140, w: 200, h: 220) }
            var sut = ScrollStitcher()
            XCTAssertEqual(sut.append(view(0)), .initial)
            XCTAssertEqual(sut.append(view(50)), .unmatched, "text at \(scale)x")
            XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(view(0)), "what is offered is the first view, as captured")
        }
    }

    func testPictureThatLoadsAfterTheFirstViewIsNeverKeptAsItsPlaceholder() throws {
        // A page with nothing fixed on it, 62 pixels longer than the view. Its one picture is a gray
        // placeholder in the first view and has loaded by the second. Near the top of the second view
        // that passed for a bar 56 pixels tall, the capture finished normally, and the image offered
        // showed the placeholder, which two of the three views contradict. A thing that is new in a bar
        // is not borne out as a bar until a later view shows it standing still: until then the image is
        // the capture as it was before. This one moves with the page in the third view, so it is no
        // bar: nothing more is stitched.
        let text = textPage(width: 800, height: 562, scale: 1.5)
        let loading = withBox(text, x: 250, y: 68, w: 300, h: 38, gray: 0.88)
        let loaded = withBox(text, x: 250, y: 68, w: 300, h: 38)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(loading, x: 0, y: 0, w: 800, h: 500)), .initial)
        XCTAssertEqual(sut.append(crop(loaded, x: 0, y: 50, w: 800, h: 500)), .moved(dx: 0, dy: 50))
        XCTAssertTrue(sut.heldBack, "the view with the loaded picture is not in the image yet")
        XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(crop(loading, x: 0, y: 0, w: 800, h: 500)),
                      "had the page ended here, the first view as captured is what is offered")
        XCTAssertEqual(sut.append(crop(loaded, x: 0, y: 62, w: 800, h: 500)), .unmatched)
        XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(crop(loading, x: 0, y: 0, w: 800, h: 500)),
                      "what is offered is the first view, as captured, and no view after it")
        // The same page with the picture there from the start is captured whole.
        var steady = ScrollStitcher()
        XCTAssertEqual(steady.append(crop(loaded, x: 0, y: 0, w: 800, h: 500)), .initial)
        XCTAssertEqual(steady.append(crop(loaded, x: 0, y: 50, w: 800, h: 500)), .moved(dx: 0, dy: 50))
        XCTAssertEqual(steady.append(crop(loaded, x: 0, y: 62, w: 800, h: 500)), .moved(dx: 0, dy: 12))
        XCTAssertFalse(steady.heldBack)
        XCTAssertTrue(pixels(try XCTUnwrap(steady.image())) == pixels(loaded))
    }

    func testSideBarBesideAPageThatScrollsDownAppearsOnce() throws {
        // A tool bar down the right of the view, with a button near its foot, while the page scrolls
        // down beside it. Each view shows the whole bar, and the image took the strip under the first
        // view from the bottom of each later one: the button came out again every step. The bar is
        // shown once, beside the view the capture began with, and beside the rest of the page the
        // image has nothing.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBox(withRightBar(crop(page, x: 0, y: y, w: 800, h: 500), columns: 72), x: 740, y: 440, w: 48, h: 28)
        }
        let (sut, reached) = automaticDown(to: 1500, viewport: 500, view: view)
        XCTAssertEqual(reached, 1500, "captured to the end")
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, 2000)
        guard actual.height == 2000 else { return }
        XCTAssertEqual(wrong(actual, page, x: 0, y: 0, w: 728, h: 2000), 0, "wrong pixels in the page beside the bar")
        XCTAssertTrue(pixels(crop(actual, x: 728, y: 0, w: 72, h: 500)) == pixels(crop(view(0), x: 728, y: 0, w: 72, h: 500)),
                      "the bar, beside the first view")
        let button = pixels(crop(view(0), x: 740, y: 440, w: 1, h: 1))[0]
        XCTAssertEqual(pixels(crop(actual, x: 728, y: 0, w: 72, h: 2000)).filter { $0 == button }.count, 48 * 28, "the button once")
        XCTAssertTrue(pixels(crop(actual, x: 740, y: 500, w: 60, h: 1500)).allSatisfy { $0 == 0 }, "nothing of the bar below the first view")
    }

    func testHeaderThatAppearsAfterTheFirstViewIsBorneOutByTheNextView() throws {
        // A header that is not there at the top of the page and is fixed once the page scrolls. In the
        // view where it first shows it could as well be a change in the page; the view after it shows
        // it standing still, and the capture goes on with it as a bar.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            let view = crop(page, x: 0, y: y, w: 800, h: 500)
            return y == 0 ? view : withHeader(view, rows: 64)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        XCTAssertEqual(sut.append(view(50)), .moved(dx: 0, dy: 50))
        XCTAssertTrue(sut.heldBack)
        XCTAssertEqual(sut.append(view(150)), .moved(dx: 0, dy: 100))
        XCTAssertFalse(sut.heldBack)
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, 650)
        XCTAssertTrue(pixels(actual) == pixels(crop(page, x: 0, y: 0, w: 800, h: 650)), "the page, with the header nowhere over it")
    }

    func testPageThatChangesWhileItScrollsIsNotStitched() throws {
        // A picture that loads after the first view was captured: the two views show different pixels
        // at the same place of the page, in the middle of the view.
        let page = textPage(width: 800, height: 2000)
        let loaded = withBox(page, x: 100, y: 220, w: 600, h: 120)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(page, x: 0, y: 0, w: 800, h: 500)), .initial)
        XCTAssertEqual(sut.append(crop(loaded, x: 0, y: 50, w: 800, h: 500)), .unmatched)
        XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(crop(page, x: 0, y: 0, w: 800, h: 500)),
                      "what is offered is the first view, as captured")
    }

    func testStepPastThePageBetweenTheBarsIsNotStitched() throws {
        // Under a 128-pixel header and over an 88-pixel bar, 84 rows of a 300-pixel view move. A step
        // of 100 would leave 16 rows of the page that no view showed clear of a bar.
        let page = textPage(width: 800, height: 2000)
        func view(_ y: Int) -> CGImage {
            withBottomBar(withHeader(crop(page, x: 0, y: y, w: 800, h: 300), rows: 128), rows: 88)
        }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        XCTAssertEqual(sut.append(view(30)), .moved(dx: 0, dy: 30))
        XCTAssertEqual(sut.append(view(130)), .unmatched)
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(actual.height, 330)
        XCTAssertEqual(wrong(actual, page, x: 0, y: 128, w: 800, h: 330 - 216), 0, "wrong pixels between the header and the bar")
    }

    func testBarsThatAppearLateOverPlacesNoOtherViewShowedAreNotStitched() throws {
        // Four views 240 pixels apart with nothing fixed in them, then a 240-pixel header and a bar
        // appear. With bars that size, 40 rows between each pair of the earlier views were only ever
        // seen where a bar now is, so no image of the page can include that view.
        let page = textPage(width: 800, height: 2000)
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(crop(page, x: 0, y: 0, w: 800, h: 500)), .initial)
        for y in [240, 480, 720] {
            XCTAssertEqual(sut.append(crop(page, x: 0, y: y, w: 800, h: 500)), .moved(dx: 0, dy: 240))
        }
        XCTAssertEqual(sut.append(withBottomBar(withHeader(crop(page, x: 0, y: 760, w: 800, h: 500), rows: 240), rows: 100)), .unmatched)
        XCTAssertTrue(pixels(try XCTUnwrap(sut.image())) == pixels(crop(page, x: 0, y: 0, w: 800, h: 1220)),
                      "what is offered is the page as far as it was captured before")
    }

    func testPlaceSeenOnlyUnderAHeaderStaysEmptyInsideThePage() throws {
        // A manual pan that starts 500 pixels down, goes right, up to the top and back left. In the
        // first 168 columns the rows under the first view's header were never seen clear of it, and
        // page was captured above and below them. The image has nothing there, as where no view went:
        // not a strip of the header.
        let page = textPage(width: 1000, height: 1000)
        func view(_ at: (x: Int, y: Int)) -> CGImage { withHeader(crop(page, x: at.x, y: at.y, w: 600, h: 500), rows: 64) }
        let path = [(0, 500), (168, 500), (168, 360), (168, 220), (168, 80), (168, 0), (0, 0)].map { (x: $0.0, y: $0.1) }
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(path[0])), .initial)
        for (last, next) in zip(path, path.dropFirst()) {
            XCTAssertEqual(sut.append(view(next)), .moved(dx: next.x - last.x, dy: next.y - last.y), "move to \(next)")
        }
        let partial = try XCTUnwrap(sut.image())
        XCTAssertEqual(partial.width, 768)
        XCTAssertEqual(partial.height, 1000)
        guard partial.width == 768, partial.height == 1000 else { return }
        XCTAssertTrue(pixels(crop(partial, x: 0, y: 500, w: 168, h: 64)).allSatisfy { $0 == 0 }, "nothing where only the header was seen")
        XCTAssertEqual(wrong(partial, page, x: 0, y: 64, w: 768, h: 436), 0, "wrong pixels above it")
        XCTAssertEqual(wrong(partial, page, x: 0, y: 564, w: 768, h: 436), 0, "wrong pixels below it")
        XCTAssertEqual(wrong(partial, page, x: 168, y: 500, w: 600, h: 64), 0, "wrong pixels beside it")
        // One more view, down the left side, shows those rows clear of the header.
        XCTAssertEqual(sut.append(view((x: 0, y: 140))), .moved(dx: 0, dy: 140))
        let actual = try XCTUnwrap(sut.image())
        XCTAssertEqual(wrong(actual, page, x: 0, y: 64, w: 768, h: 936), 0, "wrong pixels under the header")
    }

    /// Automatic downward scrolling to `end`, with steps sized as the capture sizes them, stopping at
    /// the first step that is not stitched, as the capture does. Returns how far it got.
    private func automaticDown(to end: Int, viewport: Int, view: (Int) -> CGImage) -> (ScrollStitcher, reached: Int) {
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial)
        var y = 0
        while y < end {
            let step = Int(ScrollStitcher.automaticStep(along: Double(viewport), moving: sut.movingShare.map { Double($0.height) }).rounded())
            let next = min(end, y + step)
            guard sut.append(view(next)) == .moved(dx: 0, dy: next - y) else { break }
            y = next
        }
        return (sut, y)
    }

    /// The one rule for what a capture from the top of `page` may offer: an image in which every pixel
    /// between a header of `header` rows and a bottom bar of `bar` rows is the page's, however far the
    /// capture got; or no image.
    private func exactOrNotStitched(_ sut: ScrollStitcher, page: CGImage, header: Int, bar: Int, _ name: String,
                                    file: StaticString = #filePath, line: UInt = #line) throws -> CGImage? {
        guard let actual = sut.image() else { return nil }
        XCTAssertEqual(wrong(actual, page, x: 0, y: header, w: actual.width, h: actual.height - header - bar), 0,
                       "\(name): wrong pixels between the header and the bar", file: file, line: line)
        return actual
    }

    /// Automatic downward scrolling: steps of `step` pixels to the end of a page `height` pixels tall,
    /// each of which must be stitched.
    private func scrollDown(_ height: Int, viewport: Int, step: Int, view: (Int) -> CGImage,
                            file: StaticString = #filePath, line: UInt = #line) -> ScrollStitcher {
        var sut = ScrollStitcher()
        XCTAssertEqual(sut.append(view(0)), .initial, file: file, line: line)
        var y = 0
        while y + viewport < height {
            let next = min(height - viewport, y + step)
            let update = sut.append(view(next))
            XCTAssertEqual(update, .moved(dx: 0, dy: next - y), "step to y=\(next)", file: file, line: line)
            guard update == .moved(dx: 0, dy: next - y) else { break }
            y = next
        }
        return sut
    }

    private func check(_ sut: ScrollStitcher, against expected: CGImage,
                       file: StaticString = #filePath, line: UInt = #line) throws {
        let actual = try XCTUnwrap(sut.image(), file: file, line: line)
        XCTAssertEqual(actual.width, expected.width, file: file, line: line)
        XCTAssertEqual(actual.height, expected.height, file: file, line: line)
        XCTAssertTrue(pixels(actual) == pixels(expected), "Stitched pixels differ from the fixture.", file: file, line: line)
    }

    private let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private let bitmap = CGBitmapInfo(rawValue: CGBitmapInfo.byteOrder32Little.rawValue
                                     | CGImageAlphaInfo.premultipliedFirst.rawValue)

    private func fixture(width: Int, height: Int, salt: UInt32 = 31337) -> CGImage {
        var state = salt
        var values: [UInt32] = []
        for _ in 0..<(width * height) {
            state ^= state << 13
            state ^= state >> 17
            state ^= state << 5
            values.append(0xff00_0000 | (state & 0x00ff_ffff))
        }
        return image(width: width, height: height, pixels: values)
    }

    private func image(width: Int, height: Int, pixels: [UInt32]) -> CGImage {
        let data = pixels.withUnsafeBytes { Data($0) }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                       space: colorSpace, bitmapInfo: bitmap, provider: CGDataProvider(data: data as CFData)!,
                       decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
    }

    /// Dark rendered text on a white page, as on a document or web page.
    private func textPage(width: Int, height: Int, scale: CGFloat = 1) -> CGImage {
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        let words = ["the", "quarterly", "report", "shows", "revenue", "growth", "across", "regions", "and", "teams",
                     "with", "margin", "notes", "for", "each", "customer", "segment", "plus", "forecast"]
        var seed: UInt32 = 7
        var text = ""
        for index in 0..<(height * width / 250) {
            seed = seed &* 1_103_515_245 &+ 12_345
            text += words[Int(seed >> 16) % words.count] + (index % 13 == 12 ? ".\n" : " ")
        }
        let attributes: [NSAttributedString.Key: Any] = [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, 13 * scale, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 0.1, green: 0.1, blue: 0.1, alpha: 1)
        ]
        let setter = CTFramesetterCreateWithAttributedString(NSAttributedString(string: text, attributes: attributes))
        let path = CGPath(rect: CGRect(x: 20 * scale, y: 0, width: CGFloat(width) - 40 * scale, height: CGFloat(height)),
                          transform: nil)
        CTFrameDraw(CTFramesetterCreateFrame(setter, CFRange(location: 0, length: 0), path, nil), context)
        return context.makeImage()!
    }

    /// A light gray header bar with bold labels and a bottom border over the top rows of a view.
    private func withHeader(_ view: CGImage, rows: Int, opacity: CGFloat = 1) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(red: 0.96, green: 0.96, blue: 0.96, alpha: opacity))
        context.fill(CGRect(x: 0, y: view.height - rows, width: view.width, height: rows))
        context.setFillColor(CGColor(red: 0.8, green: 0.8, blue: 0.8, alpha: 1))
        context.fill(CGRect(x: 0, y: view.height - rows, width: view.width, height: 1))
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "Home      Products      Pricing      Docs", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica-Bold" as CFString, 12, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 0.2, green: 0.2, blue: 0.2, alpha: 1)
        ]))
        context.textPosition = CGPoint(x: 20, y: CGFloat(view.height - rows) + CGFloat(rows) * 0.35)
        CTLineDraw(line, context)
        return context.makeImage()!
    }

    /// A dark bar with white labels along the bottom rows of a view, as a cookie notice or a toolbar.
    private func withBottomBar(_ view: CGImage, rows: Int) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(red: 0.18, green: 0.49, blue: 0.31, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: view.width, height: rows))
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "Cookie settings      Accept", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica-Bold" as CFString, 12, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 1, green: 1, blue: 1, alpha: 1)
        ]))
        context.textPosition = CGPoint(x: 20, y: CGFloat(rows) * 0.35)
        CTLineDraw(line, context)
        return context.makeImage()!
    }

    /// A white bar with dark labels along the bottom rows of a view, with a faint line along its top
    /// when `divider`.
    private func withWhiteBar(_ view: CGImage, rows: Int, divider: Bool) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: view.width, height: rows))
        if divider {
            context.setFillColor(CGColor(gray: 0.93, alpha: 1))
            context.fill(CGRect(x: 0, y: rows - 1, width: view.width, height: 1))
        }
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "Cookie settings     Accept", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, 13, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0.1, alpha: 1)
        ]))
        context.textPosition = CGPoint(x: 20, y: 20)
        CTLineDraw(line, context)
        return context.makeImage()!
    }

    /// A white bar with dark labels along the bottom rows of a view that stops `inset` pixels short of
    /// each side and lets the page show through unless `opacity` is 1.
    private func withCookieBar(_ view: CGImage, rows: Int, inset: Int, opacity: CGFloat = 1) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(gray: 1, alpha: opacity))
        context.fill(CGRect(x: inset, y: 0, width: view.width - 2 * inset, height: rows))
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "Cookie settings     Accept", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica" as CFString, 13, nil),
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0.1, alpha: 1)
        ]))
        context.textPosition = CGPoint(x: inset + 20, y: 20)
        CTLineDraw(line, context)
        return context.makeImage()!
    }

    /// A box over a view, its top-left corner at (`x`, `y`) from the view's top left: blue, or of a `gray`.
    private func withBox(_ view: CGImage, x: Int, y: Int, w: Int, h: Int, gray: CGFloat? = nil) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(gray.map { CGColor(gray: $0, alpha: 1) } ?? CGColor(red: 0.12, green: 0.35, blue: 0.8, alpha: 1))
        context.fill(CGRect(x: x, y: view.height - y - h, width: w, height: h))
        return context.makeImage()!
    }

    /// How many pixels of a part of the stitched image differ from the same part of the page.
    private func wrong(_ actual: CGImage, _ page: CGImage, x: Int, y: Int, w: Int, h: Int) -> Int {
        zip(pixels(crop(actual, x: x, y: y, w: w, h: h)), pixels(crop(page, x: x, y: y, w: w, h: h))).filter { $0 != $1 }.count
    }

    /// A dark bar with white labels down the right columns of a view, as a tool palette.
    private func withRightBar(_ view: CGImage, columns: Int) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(red: 0.18, green: 0.49, blue: 0.31, alpha: 1))
        context.fill(CGRect(x: view.width - columns, y: 0, width: columns, height: view.height))
        for (index, label) in ["Pen", "Text", "Shape", "Color", "Undo"].enumerated() {
            let line = CTLineCreateWithAttributedString(NSAttributedString(string: label, attributes: [
                NSAttributedString.Key(kCTFontAttributeName as String): CTFontCreateWithName("Helvetica-Bold" as CFString, 12, nil),
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 1, green: 1, blue: 1, alpha: 1)
            ]))
            context.textPosition = CGPoint(x: CGFloat(view.width - columns + 8), y: CGFloat(view.height - 40 - index * 70))
            CTLineDraw(line, context)
        }
        return context.makeImage()!
    }

    /// A 12-pixel scroll bar down the right edge whose thumb follows the scroll position.
    private func withScrollBar(_ view: CGImage, at position: Int, of length: Int) -> CGImage {
        let context = CGContext(data: nil, width: view.width, height: view.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: colorSpace, bitmapInfo: bitmap.rawValue)!
        context.draw(view, in: CGRect(x: 0, y: 0, width: view.width, height: view.height))
        context.setFillColor(CGColor(gray: 0.94, alpha: 1))
        context.fill(CGRect(x: view.width - 12, y: 0, width: 12, height: view.height))
        let thumb = view.height * view.height / length
        context.setFillColor(CGColor(gray: 0.55, alpha: 1))
        context.fill(CGRect(x: view.width - 10, y: view.height - position * view.height / length - thumb,
                            width: 8, height: thumb))
        return context.makeImage()!
    }

    private func crop(_ image: CGImage, x: Int, y: Int, w: Int, h: Int) -> CGImage {
        image.cropping(to: CGRect(x: x, y: y, width: w, height: h))!
    }

    private func pixels(_ image: CGImage) -> [UInt32] {
        var bytes = [UInt32](repeating: 0, count: image.width * image.height)
        bytes.withUnsafeMutableBytes { buffer in
            let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height,
                                    bitsPerComponent: 8, bytesPerRow: image.width * 4, space: colorSpace,
                                    bitmapInfo: bitmap.rawValue)!
            context.setBlendMode(.copy)
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        return bytes
    }
}

final class ScrollAccessPromptTests: XCTestCase {
    func testRequestAccessKeepsTheCaptureUntilAccessIsGranted() {
        var prompt = ScrollAccessPrompt()
        var trusted = false
        XCTAssertEqual(prompt.allowTitle, "Request Access")
        // macOS answers the request at once, before the user can turn access on.
        XCTAssertEqual(prompt.next(after: .allow, mode: .down, isTrusted: { trusted }, request: { false }), .ask)
        XCTAssertEqual(prompt.allowTitle, "Try Again")
        XCTAssertTrue(prompt.message.contains("Try Again"))
        XCTAssertEqual(prompt.next(after: .allow, mode: .down, isTrusted: { trusted },
                                   request: { XCTFail("asked macOS twice"); return false }), .ask)
        trusted = true
        XCTAssertEqual(prompt.next(after: .allow, mode: .down, isTrusted: { trusted }, request: { false }), .scroll(.down))
        XCTAssertEqual(prompt.next(after: .scrollManually, mode: .down, isTrusted: { false }, request: { false }),
                       .scroll(.manual))
        XCTAssertEqual(prompt.next(after: .cancel, mode: .down, isTrusted: { false }, request: { false }), .cancel)
    }
}
