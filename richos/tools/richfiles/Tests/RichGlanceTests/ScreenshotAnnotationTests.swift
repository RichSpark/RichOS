import AppKit
import ObjectiveC
import XCTest
@testable import RichGlance

@MainActor
final class ScreenshotAnnotationTests: XCTestCase {
    func testLineIsBetweenTextAndBoxAndSnapsLiveAtBothEndpoints() async throws {
        let tools = ScreenshotTool.allCases.map(\.rawValue)
        let textIndex = try XCTUnwrap(tools.firstIndex(of: "text"))
        XCTAssertEqual(Array(tools.dropFirst(textIndex).prefix(3)), ["text", "line", "box"])
        let line = try XCTUnwrap(ScreenshotTool(rawValue: "line"))
        XCTAssertEqual(line.key, "l")

        let canvas = ScreenshotCanvas(document: ScreenshotDocument(original: try bitmap(), scale: 1))
        canvas.tool = line
        prepare(canvas)
        try mouse(.leftMouseDown, at: CGPoint(x: 30, y: 40), on: canvas)
        try mouse(.leftMouseDragged, at: CGPoint(x: 130, y: 46), on: canvas)
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 130, y: 46))
        try shift(true, on: canvas)
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 130, y: 40))
        try shift(false, on: canvas)
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 130, y: 46))
        try mouse(.leftMouseDragged, at: CGPoint(x: 90, y: 95), on: canvas, flags: .shift)
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 88, y: 98))
        try mouse(.leftMouseUp, at: CGPoint(x: 90, y: 95), on: canvas, flags: .shift)
        let mark = try XCTUnwrap(canvas.selectedMark)

        try mouse(.leftMouseDown, at: mark.end, on: canvas)
        try mouse(.leftMouseDragged, at: CGPoint(x: 35, y: 130), on: canvas)
        try shift(true, on: canvas)
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 30, y: 130))
        try mouse(.leftMouseUp, at: CGPoint(x: 35, y: 130), on: canvas, flags: .shift)

        try mouse(.leftMouseDown, at: mark.start, on: canvas)
        try mouse(.leftMouseDragged, at: CGPoint(x: 200, y: 126), on: canvas, flags: .shift)
        try mouse(.leftMouseUp, at: CGPoint(x: 200, y: 126), on: canvas, flags: .shift)
        XCTAssertEqual(canvas.selectedMark?.start, CGPoint(x: 200, y: 130))
        XCTAssertEqual(canvas.selectedMark?.end, CGPoint(x: 30, y: 130))
    }

    func testThicknessControlAndShortcutProduceOneExportPixelAtTwoX() async throws {
        try withStore { store in
            let editor = try makeEditor(store, scale: 2)
            let canvas = try XCTUnwrap(editor.canvas)
            let box = makeMark(.box, from: CGPoint(x: 30, y: 26), to: CGPoint(x: 220, y: 90))
            canvas.document.edits.marks = [box]
            canvas.selected = box.id
            canvas.refresh()
            let button = controls(in: try XCTUnwrap(editor.window.contentView)).first {
                $0.title.hasPrefix("Thickness") && !$0.isHidden
            }
            XCTAssertNotNil(button)
            let background = try pixel(try flatten(canvas), x: 63, y: 20)
            for _ in 0..<5 { try press("-", on: editor) }
            let singlePixelBox = try flatten(canvas)
            XCTAssertTrue(isRed(try pixel(singlePixelBox, x: 63, y: 26)))
            XCTAssertEqual(try pixel(singlePixelBox, x: 63, y: 25), background)
            XCTAssertEqual(try pixel(singlePixelBox, x: 63, y: 27), background)
            canvas.undo()
            XCTAssertTrue(isRed(try pixel(try flatten(canvas), x: 63, y: 25)))
            canvas.redo()
            XCTAssertEqual(try pixel(try flatten(canvas), x: 63, y: 25), background)

            let line = makeMark(try XCTUnwrap(ScreenshotTool(rawValue: "line")),
                                from: CGPoint(x: 36, y: 112), to: CGPoint(x: 268, y: 112))
            canvas.document.edits.marks = [line]
            canvas.selected = line.id
            canvas.refresh()
            for _ in 0..<3 { try press("-", on: editor) }
            let singlePixelLine = try flatten(canvas)
            XCTAssertTrue(isRed(try pixel(singlePixelLine, x: 70, y: 112)))
            for y in [110, 111, 113, 114] {
                XCTAssertEqual(try pixel(singlePixelLine, x: 70, y: y), background)
            }
            XCTAssertEqual(try pixel(singlePixelLine, x: 35, y: 112), background)
            XCTAssertEqual(try pixel(singlePixelLine, x: 268, y: 112), background)
        }
    }

    func testBoxAndArrowHaveNoWhiteStrokeInExport() async throws {
        var document = ScreenshotDocument(original: try bitmap(), scale: 1)
        let box = makeMark(.box, from: CGPoint(x: 25, y: 22), to: CGPoint(x: 240, y: 65))
        let arrow = makeMark(.arrow, from: CGPoint(x: 36, y: 112), to: CGPoint(x: 268, y: 112))
        document.edits.marks = [box, arrow]
        let image = try XCTUnwrap(ScreenshotRenderer.flattened(document))
        for (name, x, rows) in [("Box", 100, 15..<30), ("Arrow", 100, 100..<126)] {
            let white = try rows.filter { try pixel(image, x: x, y: $0) == [255, 255, 255] }
            XCTAssertTrue(white.isEmpty, "\(name) exported a white stroke at rows \(white)")
        }
    }

    func testEachBoxSideChangesOnlyItsOwnAxis() async throws {
        try checkSideGrabbers(of: .box)
    }

    func testEachRedactSideChangesOnlyItsOwnAxis() async throws {
        try checkSideGrabbers(of: .redact)
    }

    func testArrowGeometryMatchesBothSourceSizesAndScales() async throws {
        for (large, shaft, head) in [(false, 6, 18), (true, 10, 30)] {
            var document = ScreenshotDocument(original: try bitmap(gray: 1), scale: 1)
            document.edits.marks = [
                ScreenshotMark(tool: .arrow, start: CGPoint(x: 36, y: 48), end: CGPoint(x: 268, y: 48), large: large)
            ]
            let a = try XCTUnwrap(ScreenshotRenderer.flattened(document))
            let rows = try (25..<70).filter { try isRed(pixel(a, x: 100, y: $0)) }
            XCTAssertEqual(rows, Array((48 - shaft / 2)..<(48 + shaft / 2)))
            let tail = try (33..<40).filter { try isRed(pixel(a, x: $0, y: 48 - shaft / 2)) }
            XCTAssertEqual(tail, [36, 37, 38, 39])
            let headRows = try (25..<70).filter { try isRed(pixel(a, x: 268 - head, y: $0)) }
            XCTAssertEqual(headRows, Array((48 - head / 2)..<(48 + head / 2)))
            XCTAssertTrue(try isRed(pixel(a, x: 259, y: 48)))
            XCTAssertFalse(try isRed(pixel(a, x: 259, y: 41)))
            XCTAssertFalse(try isRed(pixel(a, x: 268, y: 48)))
            XCTAssertLessThan(try pixel(a, x: 100, y: 55)[0], 235, "Shadow below the shaft")
            XCTAssertGreaterThan(try pixel(a, x: 100, y: 38)[0], 250, "No shadow above the shaft")
            XCTAssertLessThan(try pixel(a, x: 270, y: 52)[0], 250, "Shadow to the right of the tip")

            let retina = ScreenshotDocument(original: try bitmap(), scale: 2, edits: document.edits)
            let b = try XCTUnwrap(ScreenshotRenderer.flattened(retina))
            XCTAssertEqual(try (25..<70).filter { try isRed(pixel(b, x: 100, y: $0)) }.count, 2 * shaft)
        }
    }

    func testRound108ArrowsAndStepsComeInSmallMediumAndLarge() async throws {
        // Arrow shafts 6, 10 and 24 px at 1x, each with a head 3 shafts wide and 3 long, as in round 108.
        for (size, shaft) in [(ScreenshotMarkSize.small, 6), (.medium, 10), (.large, 24)] {
            var document = ScreenshotDocument(original: try bitmap(gray: 1), scale: 1)
            var arrow = ScreenshotMark(tool: .arrow, start: CGPoint(x: 20, y: 75), end: CGPoint(x: 280, y: 75))
            arrow.markSize = size
            document.edits.marks = [arrow]
            let image = try XCTUnwrap(ScreenshotRenderer.flattened(document))
            let rows = try (0..<150).filter { try isRed(pixel(image, x: 100, y: $0)) }
            XCTAssertEqual(rows, Array((75 - shaft / 2)..<(75 + shaft / 2)), "\(size) shaft")
            let head = 3 * shaft
            let headRows = try (0..<150).filter { try isRed(pixel(image, x: 280 - head, y: $0)) }
            XCTAssertEqual(headRows, Array((75 - head / 2)..<(75 + head / 2)), "\(size) head")
        }
        // Step circles 30, 45 and 120 px: Large is 4 times Small.
        for (size, diameter) in [(ScreenshotMarkSize.small, 30.0), (.medium, 45), (.large, 120)] {
            var document = ScreenshotDocument(original: try bitmap(gray: 1), scale: 1)
            var step = ScreenshotMark(tool: .steps, start: CGPoint(x: 149, y: 75), end: CGPoint(x: 149, y: 75))
            step.markSize = size
            document.edits.marks = [step]
            let image = try XCTUnwrap(ScreenshotRenderer.flattened(document))
            let top = Int((75 - diameter / 2).rounded())
            XCTAssertTrue(try isRed(pixel(image, x: 149, y: top + 3)), "\(size) step inside its edge")
            XCTAssertFalse(try isRed(pixel(image, x: 149, y: top - 3)), "\(size) step outside its edge")
        }

        // A mark saved before round 108 keeps its thickness: Regular reads as Small, Large as Medium.
        let saved = ScreenshotMark(tool: .arrow, start: .zero, end: CGPoint(x: 90, y: 0), large: true)
        let data = try JSONEncoder().encode(saved)
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains("\"size\""))
        XCTAssertEqual(try JSONDecoder().decode(ScreenshotMark.self, from: data).markSize, .medium)
        XCTAssertEqual(ScreenshotMark(tool: .steps, start: .zero, end: .zero).markSize, .small)

        let home = FileManager.default.temporaryDirectory.appendingPathComponent("glance-108-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        // A step size chosen before round 108 (Large) reads as Medium.
        try JSONEncoder().encode(ScreenshotMarkStyle(large: true)).write(to: home.appendingPathComponent("annotation-steps.json"))
        let store = ScreenshotStore(root: home)
        try store.setHistory(false)
        let editor = try makeEditor(store)
        let canvas = try XCTUnwrap(editor.canvas)
        let root = try XCTUnwrap(editor.window.contentView)
        func shown() -> [String] { controls(in: root).filter { !$0.isHiddenOrHasHiddenAncestor }.map(\.title) }
        // Nobody picked an arrow size: Medium, with Small, Medium and Large on offer. Text keeps Regular and Large.
        canvas.tool = .arrow
        XCTAssertEqual(canvas.size, .medium)
        XCTAssertTrue(["Small", "Medium", "Large"].allSatisfy(shown().contains), "\(shown())")
        XCTAssertFalse(shown().contains("Regular"))
        canvas.tool = .text
        XCTAssertEqual(canvas.size, .small)
        XCTAssertTrue(["Regular", "Large"].allSatisfy(shown().contains), "\(shown())")
        XCTAssertFalse(shown().contains("Medium"))
        canvas.tool = .steps
        XCTAssertEqual(canvas.size, .medium)
        XCTAssertTrue(["Small", "Medium", "Large"].allSatisfy(shown().contains), "\(shown())")

        // Picking Large with the Arrow tool draws the next arrow Large and remembers it.
        canvas.tool = .arrow
        prepare(canvas)
        let large = try XCTUnwrap(controls(in: root).first { $0.title == "Large" && !$0.isHiddenOrHasHiddenAncestor })
        large.performClick(nil)
        XCTAssertEqual(canvas.size, .large)
        try drag(from: CGPoint(x: 30, y: 75), to: CGPoint(x: 260, y: 75), on: canvas)
        XCTAssertEqual(canvas.selectedMark?.markSize, .large)
        let reopened = try XCTUnwrap(try makeEditor(store).canvas)
        reopened.tool = .arrow
        XCTAssertEqual(reopened.size, .large)
        reopened.tool = .text
        XCTAssertEqual(reopened.size, .small)
    }

    func testRound108ASizeChosenAfterPlacingAStepIsForTheNextStep() async throws {
        try withStore { store in
            let editor = try makeEditor(store)
            let canvas = try XCTUnwrap(editor.canvas)
            let root = try XCTUnwrap(editor.window.contentView)
            @MainActor func choose(_ title: String) throws {
                try XCTUnwrap(controls(in: root).first { $0.title == title && !$0.isHiddenOrHasHiddenAncestor })
                    .performClick(nil)
            }
            @MainActor func sizes(_ tool: ScreenshotTool) -> [ScreenshotMarkSize] {
                canvas.document.edits.marks.filter { $0.tool == tool }.map(\.markSize)
            }
            prepare(canvas)
            // Steps, without picking the tool again between placements: the placed step stays Small.
            canvas.tool = .steps
            try choose("Small")
            try mouse(.leftMouseDown, at: CGPoint(x: 60, y: 75), on: canvas)
            try mouse(.leftMouseUp, at: CGPoint(x: 60, y: 75), on: canvas)
            XCTAssertNil(canvas.selectedMark)
            try choose("Large")
            try mouse(.leftMouseDown, at: CGPoint(x: 200, y: 75), on: canvas)
            try mouse(.leftMouseUp, at: CGPoint(x: 200, y: 75), on: canvas)
            XCTAssertEqual(sizes(.steps), [.small, .large])

            // A step selected on purpose through Select still takes the size chosen.
            canvas.tool = .select
            try mouse(.leftMouseDown, at: CGPoint(x: 60, y: 75), on: canvas)
            try mouse(.leftMouseUp, at: CGPoint(x: 60, y: 75), on: canvas)
            XCTAssertEqual(canvas.selectedMark?.tool, .steps)
            try choose("Medium")
            XCTAssertEqual(sizes(.steps), [.medium, .large])

            // Arrows as round 108 has them: a newly drawn arrow stays selected and takes the size chosen.
            canvas.tool = .arrow
            try choose("Small")
            try drag(from: CGPoint(x: 30, y: 30), to: CGPoint(x: 260, y: 30), on: canvas)
            try choose("Large")
            XCTAssertEqual(sizes(.arrow), [.large])
        }
    }

    func testDefaultsFollowTheEditedToolIntoANewEditAndReopenedEditor() async throws {
        try withStore { store in
            let editor = try makeEditor(store)
            let canvas = try XCTUnwrap(editor.canvas)
            let choices: [(ScreenshotTool, ScreenshotInk)] = [
                (.arrow, .blue), (.text, .black), (.box, .yellow), (.highlight, .blue), (.steps, .black),
            ]
            for (tool, color) in choices {
                let mark = makeMark(tool, from: CGPoint(x: 30, y: 26), to: CGPoint(x: 220, y: 90))
                canvas.tool = .select
                canvas.document.edits.marks = [mark]
                canvas.selected = mark.id
                canvas.setStyle(ink: color, size: tool == .arrow ? .small : nil)
                if tool == .box { for _ in 0..<3 { try press("-", on: editor) } }
                XCTAssertEqual(canvas.selectedMark?.ink, color)
            }
            // Persist even if capture history is off.
            XCTAssertFalse(store.keepsHistory)
            canvas.document.edits.marks = []
            canvas.selected = nil
            let next = ScreenshotDocument(original: try bitmap(), scale: 1)
            editor.changeDocument(next)
            let newEdit = try XCTUnwrap(editor.canvas)
            XCTAssertEqual(newEdit.document.id, next.id)
            let reopened = try makeEditor(store)
            let relaunchedEdit = try XCTUnwrap(reopened.canvas)
            for edit in [newEdit, relaunchedEdit] {
                for (tool, color) in choices {
                    edit.tool = tool
                    XCTAssertEqual(edit.ink, color, tool.title)
                    if tool == .arrow { XCTAssertEqual(edit.size, .small) }
                }
            }

            let line = try XCTUnwrap(ScreenshotTool(rawValue: "line"))
            newEdit.tool = line
            newEdit.setStyle(ink: .yellow)
            try press("-", on: editor)
            let fresh = try makeEditor(store)
            let loaded = try XCTUnwrap(fresh.canvas)
            for tool in [line, .box] {
                loaded.tool = tool
                loaded.selected = nil
                XCTAssertEqual(loaded.ink, .yellow)
                prepare(loaded)
                try drag(from: CGPoint(x: 44, y: 36), to: CGPoint(x: 250, y: 100), on: loaded)
                XCTAssertEqual(loaded.selectedMark?.ink, .yellow)
                let button = controls(in: try XCTUnwrap(fresh.window.contentView)).first {
                    $0.title.hasPrefix("Thickness") && !$0.isHidden
                }
                XCTAssertEqual(button?.accessibilityValue() as? String, "1 px", tool.title)
            }
        }
    }

    func testTextAndStepsRememberSizeAcrossCapturesAndEditorRecreation() async throws {
        try withStore { store in
            let editor = try makeEditor(store)
            let canvas = try XCTUnwrap(editor.canvas)
            for tool in [ScreenshotTool.text, .steps] {
                canvas.tool = .select
                let mark = makeMark(tool, from: CGPoint(x: 30, y: 30), to: CGPoint(x: 130, y: 70))
                canvas.document.edits.marks = [mark]
                canvas.selected = mark.id
                canvas.setStyle(ink: .blue, size: .medium)
                XCTAssertEqual(canvas.selectedMark?.markSize, .medium)
            }
            canvas.document.edits.marks = []
            canvas.selected = nil
            editor.changeDocument(ScreenshotDocument(original: try bitmap(), scale: 1))
            let sameEditor = try XCTUnwrap(editor.canvas)
            let reopened = try makeEditor(store)
            let newEditor = try XCTUnwrap(reopened.canvas)
            for fresh in [sameEditor, newEditor] {
                for tool in [ScreenshotTool.text, .steps] {
                    fresh.tool = tool
                    XCTAssertEqual(fresh.size, .medium, tool.title)
                    XCTAssertEqual(fresh.ink, .blue, tool.title)
                }
            }
            newEditor.tool = .text
            newEditor.setStyle(size: .small)
            let third = try makeEditor(store)
            let latest = try XCTUnwrap(third.canvas)
            latest.tool = .text
            XCTAssertEqual(latest.size, .small)
            XCTAssertEqual(latest.ink, .blue)
            XCTAssertEqual(store.annotationStyle(for: .text).markSize, .small)
            latest.tool = .steps
            XCTAssertEqual(latest.size, .medium)
        }
    }

    func testCanvasLeavesTheCursorAloneOverAnActiveCallout() async throws {
        // Intercept cursor writes in this test process. No cursor change reaches a display.
        var writes: [NSCursor] = []
        let method = try XCTUnwrap(class_getInstanceMethod(NSCursor.self, #selector(NSCursor.set)))
        let capture: @convention(block) (NSCursor) -> Void = { writes.append($0) }
        let replacement = imp_implementationWithBlock(capture)
        let original = method_setImplementation(method, replacement)
        defer {
            method_setImplementation(method, original)
            imp_removeBlock(replacement)
        }

        let canvas = ScreenshotCanvas(document: ScreenshotDocument(original: try bitmap(), scale: 1))
        prepare(canvas)
        var text = makeMark(.text, from: CGPoint(x: 30, y: 30), to: CGPoint(x: 180, y: 90))
        text.text = "Editable text"
        canvas.document.edits.marks = [text]
        canvas.selected = text.id
        canvas.startCallout()
        let field = try XCTUnwrap(canvas.subviews.compactMap { $0 as? NSTextView }.first)
        let inside = CGPoint(x: field.frame.midX, y: field.frame.midY)
        let hover = try moveEvent(at: inside, on: canvas)

        writes.removeAll()
        canvas.cursorUpdate(with: hover)
        canvas.mouseMoved(with: hover)
        XCTAssertTrue(writes.isEmpty, "The native text field owns the cursor inside its frame")

        canvas.mouseMoved(with: try moveEvent(at: CGPoint(x: 15, y: 15), on: canvas))
        XCTAssertEqual(writes, [.arrow], "Leaving the field restores the canvas cursor")

        canvas.finishCallout()
        writes.removeAll()
        canvas.mouseMoved(with: hover)
        XCTAssertEqual(writes, [.arrow], "The canvas owns the cursor after editing ends")
    }

    private func moveEvent(at point: CGPoint, on canvas: ScreenshotCanvas) throws -> NSEvent {
        try XCTUnwrap(NSEvent.mouseEvent(with: .mouseMoved, location: canvas.convert(point, to: nil),
                                        modifierFlags: [], timestamp: 0, windowNumber: 0,
                                        context: nil, eventNumber: 0, clickCount: 0, pressure: 0))
    }

    func testRedactionIsSolidAndHasNoWhiteBorderAtAnyInset() async throws {
        var document = ScreenshotDocument(original: try bitmap(), scale: 1)
        document.edits.marks = [makeMark(.redact, from: CGPoint(x: 30, y: 40), to: CGPoint(x: 100, y: 90))]
        let flat = try XCTUnwrap(ScreenshotRenderer.flattened(document))
        XCTAssertEqual(try pixel(flat, x: 60, y: 39), [192, 192, 192])
        let inside = try (40..<90).map { try pixel(flat, x: 60, y: $0) }
        XCTAssertTrue(inside.allSatisfy { $0 == [12, 19, 34] }, "Redaction must be uniform from edge to edge")
    }

    private func checkSideGrabbers(of tool: ScreenshotTool) throws {
        let canvas = ScreenshotCanvas(document: ScreenshotDocument(original: try bitmap(), scale: 1))
        prepare(canvas)
        let mark = makeMark(tool, from: CGPoint(x: 40, y: 35), to: CGPoint(x: 210, y: 105))
        let cases = [
            (CGPoint(x: 210, y: 70), CGPoint(x: 247, y: 92), CGRect(x: 40, y: 35, width: 207, height: 70)),
            (CGPoint(x: 125, y: 35), CGPoint(x: 104, y: 20), CGRect(x: 40, y: 20, width: 170, height: 85)),
            (CGPoint(x: 40, y: 70), CGPoint(x: 22, y: 47), CGRect(x: 22, y: 35, width: 188, height: 70)),
            (CGPoint(x: 125, y: 105), CGPoint(x: 141, y: 128), CGRect(x: 40, y: 35, width: 170, height: 93)),
        ]
        for (from, to, expected) in cases {
            canvas.document.edits.marks = [mark]
            canvas.selected = mark.id
            try drag(from: from, to: to, on: canvas)
            XCTAssertEqual(canvas.selectedMark?.rect, expected, "\(tool.title) side at \(from)")
        }
    }

    private func withStore(_ execute: (ScreenshotStore) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("glance-annotation-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = ScreenshotStore(root: root)
        try store.setHistory(false)
        try execute(store)
    }

    private func makeEditor(_ store: ScreenshotStore, scale: CGFloat = 1) throws -> ScreenshotEditor {
        _ = NSApplication.shared
        let editor = ScreenshotEditor(store: store)
        editor.changeDocument(ScreenshotDocument(original: try bitmap(), scale: scale))
        return editor
    }

    private func controls(in root: NSView) -> [NSButton] {
        (root as? NSButton).map { [$0] } ?? root.subviews.flatMap { controls(in: $0) }
    }

    private func press(_ key: String, on editor: ScreenshotEditor) throws {
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
                                                  timestamp: 0, windowNumber: editor.window.windowNumber,
                                                  context: nil, characters: key, charactersIgnoringModifiers: key,
                                                  isARepeat: false, keyCode: key == "-" ? 27 : 24))
        editor.window.sendEvent(event)
    }

    private func prepare(_ canvas: ScreenshotCanvas) {
        canvas.layoutImage(in: CGSize(width: 398, height: 250), zoom: .actual)
    }

    private func mouse(_ type: NSEvent.EventType, at source: CGPoint, on canvas: ScreenshotCanvas,
                       flags: NSEvent.ModifierFlags = []) throws {
        let size = canvas.document.bounds.size
        let view = CGPoint(x: ((canvas.frame.width - size.width * canvas.factor) / 2).rounded() + source.x * canvas.factor,
                           y: ((canvas.frame.height - size.height * canvas.factor) / 2).rounded() + source.y * canvas.factor)
        let event = try XCTUnwrap(NSEvent.mouseEvent(with: type, location: canvas.convert(view, to: nil),
                                                    modifierFlags: flags, timestamp: 0,
                                                    windowNumber: canvas.window?.windowNumber ?? 0,
                                                    context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        switch type {
        case .leftMouseDown: canvas.mouseDown(with: event)
        case .leftMouseDragged: canvas.mouseDragged(with: event)
        case .leftMouseUp: canvas.mouseUp(with: event)
        default: XCTFail("Unexpected mouse event")
        }
    }

    private func shift(_ held: Bool, on canvas: ScreenshotCanvas) throws {
        let flags = held ? CGEventFlags.maskShift : []
        let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: 56, keyDown: held))
        event.type = .flagsChanged
        event.flags = flags
        canvas.flagsChanged(with: try XCTUnwrap(NSEvent(cgEvent: event)))
    }

    private func drag(from: CGPoint, to: CGPoint, on canvas: ScreenshotCanvas) throws {
        try mouse(.leftMouseDown, at: from, on: canvas)
        try mouse(.leftMouseDragged, at: to, on: canvas)
        try mouse(.leftMouseUp, at: to, on: canvas)
    }

    private func makeMark(_ tool: ScreenshotTool, from: CGPoint, to: CGPoint) -> ScreenshotMark {
        ScreenshotMark(tool: tool, start: from, end: to)
    }

    private func bitmap(gray: CGFloat = 192.0 / 255) throws -> CGImage {
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: 298, height: 150, bitsPerComponent: 8,
                                              bytesPerRow: 0, space: space,
                                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(red: gray, green: gray, blue: gray, alpha: 1)
        context.fill(CGRect(x: 0, y: 0, width: context.width, height: context.height))
        return try XCTUnwrap(context.makeImage())
    }

    private func flatten(_ canvas: ScreenshotCanvas) throws -> CGImage {
        try XCTUnwrap(ScreenshotRenderer.flattened(canvas.document))
    }

    private func pixel(_ image: CGImage, x: Int, y: Int) throws -> [UInt8] {
        let cut = try XCTUnwrap(image.cropping(to: CGRect(x: x, y: y, width: 1, height: 1)))
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        var bytes = [UInt8](repeating: 0, count: 4)
        try bytes.withUnsafeMutableBytes { storage in
            let context = try XCTUnwrap(CGContext(data: storage.baseAddress, width: 1, height: 1, bitsPerComponent: 8,
                                                  bytesPerRow: 4, space: space,
                                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(cut, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        }
        return Array(bytes.prefix(3))
    }

    private func isRed(_ pixel: [UInt8]) -> Bool { Int(pixel[0]) - Int(pixel[1]) > 100 }
}
