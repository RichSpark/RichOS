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
        try await read(preparing: { input })
    }

    static func read(preparing makeInput: @escaping @Sendable () -> Input?) async throws -> [Line] {
        let worker = Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            guard let input = makeInput() else { throw CocoaError(.fileReadUnknown) }
            return try recognize(input)
        }
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
        return (request.results ?? []).flatMap { observation -> [Line] in
            guard let candidate = observation.topCandidates(1).first, !candidate.string.isEmpty else { return [] }
            let line = Line(text: candidate.string, bounds: pixels(observation.boundingBox, in: frame))
            if !input.redactions.contains(where: { $0.intersects(line.bounds) }) { return [line] }

            var words: [Line] = []
            var result: [Line] = []
            for word in candidate.string.split(whereSeparator: \.isWhitespace) {
                let range = word.startIndex..<word.endIndex
                let box = (try? candidate.boundingBox(for: range)).map { pixels($0.boundingBox, in: frame) }
                if let box, !input.redactions.contains(where: { $0.intersects(box) }) {
                    words.append(Line(text: String(word), bounds: box))
                } else if !words.isEmpty {
                    result.append(join(words))
                    words.removeAll()
                }
            }
            if !words.isEmpty { result.append(join(words)) }
            return result
        }
    }

    private static func pixels(_ box: CGRect, in frame: CGSize) -> CGRect {
        CGRect(x: box.minX * frame.width, y: (1 - box.maxY) * frame.height,
               width: box.width * frame.width, height: box.height * frame.height)
    }

    private static func join(_ words: [Line]) -> Line {
        Line(text: words.map(\.text).joined(separator: " "),
             bounds: words.reduce(CGRect.null) { $0.union($1.bounds) })
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
