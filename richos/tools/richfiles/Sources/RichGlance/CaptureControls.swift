import AppKit

enum CaptureMode: String, CaseIterable {
    case region, window, display
    var title: String { rawValue.capitalized }
    var next: CaptureMode {
        let modes = Self.allCases
        return modes[((modes.firstIndex(of: self) ?? 0) + 1) % modes.count]
    }
}

@MainActor
final class CaptureControls: NSStackView {
    private let modes = NSSegmentedControl(labels: CaptureMode.allCases.map(\.title), trackingMode: .selectOne,
                                          target: nil, action: nil)
    private let delays = NSPopUpButton()
    private let pointer = NSButton(checkboxWithTitle: "Pointer", target: nil, action: nil)
    private let cancel = NSButton(title: "Cancel", target: nil, action: nil)
    private static let seconds = [0, 3, 5, 10]

    var modeChanged: ((CaptureMode) -> Void)?
    var delayChanged: ((Int) -> Void)?
    var pointerChanged: ((Bool) -> Void)?
    var canceled: (() -> Void)?

    init() {
        super.init(frame: .zero)
        orientation = .horizontal
        alignment = .centerY
        spacing = 12
        edgeInsets = NSEdgeInsets(top: 7, left: 12, bottom: 7, right: 12)
        modes.target = self
        modes.action = #selector(pickMode)
        modes.setAccessibilityLabel("Capture target")
        delays.addItems(withTitles: Self.seconds.map { $0 == 0 ? "Delay: None" : "Delay: \($0) s" })
        delays.target = self
        delays.action = #selector(pickDelay)
        delays.setAccessibilityLabel("Capture delay")
        pointer.target = self
        pointer.action = #selector(pickPointer)
        cancel.target = self
        cancel.action = #selector(cancelPressed)
        for control in [modes, delays, pointer, cancel] as [NSControl] {
            control.font = .systemFont(ofSize: 16, weight: .medium)
            control.controlSize = .large
            addArrangedSubview(control)
        }
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.borderWidth = 1
        updateColors()
    }

    required init?(coder: NSCoder) { nil }

    func update(mode: CaptureMode, delay: Int, includesPointer: Bool) {
        modes.selectedSegment = CaptureMode.allCases.firstIndex(of: mode) ?? 0
        delays.selectItem(at: Self.seconds.firstIndex(of: delay) ?? 0)
        pointer.state = includesPointer ? .on : .off
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        let colors = ScreenshotChrome.colors(for: self)
        layer?.backgroundColor = colors.surface.cgColor
        layer?.borderColor = colors.gold.cgColor
    }

    @objc private func pickMode() {
        guard CaptureMode.allCases.indices.contains(modes.selectedSegment) else { return }
        modeChanged?(CaptureMode.allCases[modes.selectedSegment])
    }
    @objc private func pickDelay() {
        guard Self.seconds.indices.contains(delays.indexOfSelectedItem) else { return }
        delayChanged?(Self.seconds[delays.indexOfSelectedItem])
    }
    @objc private func pickPointer() { pointerChanged?(pointer.state == .on) }
    @objc private func cancelPressed() { canceled?() }
}
