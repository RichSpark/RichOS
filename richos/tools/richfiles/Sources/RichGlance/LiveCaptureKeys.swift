import Carbon

/// The source app has keyboard focus during a timer or a live scroll. Register the few
/// capture keys for just that interval, so the source app remains usable.
@MainActor
final class LiveCaptureKeys {
    private static let signature: UInt32 = 0x5247434B
    private var handler: EventHandlerRef?
    private var registrations: [EventHotKeyRef] = []
    private let actions: [UInt32: () -> Void]

    init(escape: @escaping () -> Void, stop: (() -> Void)? = nil) {
        var actions = [UInt32(kVK_Escape): escape]
        if let stop { actions[UInt32(kVK_Return)] = stop }
        self.actions = actions
        var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let target = GetApplicationEventTarget()
        let context = Unmanaged.passUnretained(self).toOpaque()
        guard InstallEventHandler(target, { _, event, pointer in
            guard let event, let pointer else { return OSStatus(eventNotHandledErr) }
            var hotKey = EventHotKeyID()
            let result = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                           nil, MemoryLayout<EventHotKeyID>.size, nil, &hotKey)
            guard result == noErr else { return result }
            return MainActor.assumeIsolated {
                guard hotKey.signature == LiveCaptureKeys.signature else { return OSStatus(eventNotHandledErr) }
                let keys = Unmanaged<LiveCaptureKeys>.fromOpaque(pointer).takeUnretainedValue()
                guard let action = keys.actions[hotKey.id] else { return OSStatus(eventNotHandledErr) }
                // The callback may end the capture and release these registrations.
                Task { @MainActor in action() }
                return noErr
            }
        }, 1, &eventType, context, &handler) == noErr else { return }
        for code in actions.keys {
            var registration: EventHotKeyRef?
            let id = EventHotKeyID(signature: Self.signature, id: code)
            if RegisterEventHotKey(code, 0, id, target, 0, &registration) == noErr, let registration {
                registrations.append(registration)
            }
        }
    }

    deinit {
        for registration in registrations { UnregisterEventHotKey(registration) }
        if let handler { RemoveEventHandler(handler) }
    }
}
