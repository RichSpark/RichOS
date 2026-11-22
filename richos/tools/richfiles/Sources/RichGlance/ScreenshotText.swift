import AppKit
import Vision

enum ScreenshotText {
    struct Line: Sendable, Equatable {
        let text: String
        /// Pixel coordinates in the exported image, measured from the top left.
        let bounds: CGRect

        func matches(_ query: String) -> Bool {
            text.range(of: query, options: [.caseInsensitive, .diacriticInsensitive]) != nil
        }
    }

    struct Input: Sendable {
        let image: CGImage
        let redactions: [CGRect]

        init(image: CGImage, redactions: [CGRect] = []) {
            self.image = image
            self.redactions = redactions
        }

        init?(document: ScreenshotDocument) {
            guard let image = ScreenshotRenderer.flattened(document) else { return nil }
            let crop = document.crop
            let redactions = document.edits.marks.filter { $0.tool == .redact }.compactMap { mark -> CGRect? in
                let overlap = mark.rect.integral.intersection(crop)
                guard !overlap.isEmpty else { return nil }
                return overlap.offsetBy(dx: -crop.minX, dy: -crop.minY)
            }
            self.init(image: image, redactions: redactions)
        }
    }

    static func read(_ input: Input) async throws -> [Line] {
        let worker = Task.detached(priority: .userInitiated) { try recognize(input) }
        return try await withTaskCancellationHandler { try await worker.value } onCancel: { worker.cancel() }
    }

    static func recognize(_ input: Input) throws -> [Line] {
        try Task.checkCancellation()
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: input.image, orientation: .up).perform([request])
        try Task.checkCancellation()
        let frame = CGSize(width: input.image.width, height: input.image.height)
        let lines = (request.results ?? []).compactMap { observation -> Line? in
            guard let text = observation.topCandidates(1).first?.string, !text.isEmpty else { return nil }
            let box = observation.boundingBox
            let bounds = CGRect(x: box.minX * frame.width, y: (1 - box.maxY) * frame.height,
                                width: box.width * frame.width, height: box.height * frame.height)
            return Line(text: text, bounds: bounds)
        }
        return visibleLines(lines, redactions: input.redactions)
    }

    static func visibleLines(_ lines: [Line], redactions: [CGRect]) -> [Line] {
        lines.filter { line in
            // Vision can split a single line into fragments around an opaque block.
            // Exclude the whole affected line so a partial recognition cannot reconstruct it.
            !redactions.contains { cover in
                cover.minY < line.bounds.maxY && cover.maxY > line.bounds.minY
            }
        }
    }

    @MainActor
    static func copy(_ lines: [Line]) -> Bool {
        guard !lines.isEmpty else { return false }
        let value = lines.map(\.text).joined(separator: "\n")
        let clipboard = NSPasteboard.general
        clipboard.clearContents()
        return clipboard.setString(value, forType: .string)
    }
}
