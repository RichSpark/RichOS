import Carbon
import Darwin

func fail(_ message: String) -> Never {
    fputs(message + "\n", stderr)
    exit(1)
}

// The test must never change the host's keyboard.
var virtualMachine: Int32 = 0
var size = MemoryLayout<Int32>.size
guard sysctlbyname("kern.hv_vmm_present", &virtualMachine, &size, nil, 0) == 0, virtualMachine == 1 else {
    fail("Input sources may only be changed in the test guest")
}

func sourceID(_ source: TISInputSource) -> String? {
    guard let property = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) else { return nil }
    return Unmanaged<CFString>.fromOpaque(property).takeUnretainedValue() as String
}

if CommandLine.arguments.count == 2 {
    let id = CommandLine.arguments[1]
    let filter = [kTISPropertyInputSourceID as String: id] as CFDictionary
    guard let matches = TISCreateInputSourceList(filter, true)?.takeRetainedValue() as? [TISInputSource],
          let source = matches.first else { fail("Input source not found: \(id)") }
    let enabled = TISGetInputSourceProperty(source, kTISPropertyInputSourceIsEnabled)
        .map { CFBooleanGetValue(Unmanaged<CFBoolean>.fromOpaque($0).takeUnretainedValue()) } ?? false
    guard enabled || TISEnableInputSource(source) == noErr else { fail("Could not enable \(id)") }
    let result = TISSelectInputSource(source)
    guard result == noErr else { fail("Could not select \(id) (\(result))") }
} else if CommandLine.arguments.count != 1 {
    fail("Usage: guest-input-source [input-source-id]")
}

guard let current = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(), let id = sourceID(current) else {
    fail("The guest did not report its input source")
}
print(id)
