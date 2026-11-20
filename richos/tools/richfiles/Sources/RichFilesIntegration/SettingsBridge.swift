import Foundation
import RichFiles

@_cdecl("richfiles_settings")
public func richFilesSettings(_ option: Int32, _ enabled: Int32,
                              _ callback: @escaping @convention(c) (UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?) -> Void,
                              _ context: UnsafeMutableRawPointer?) {
    Task { @MainActor in
        guard let data = await SharedBrowserSettings.update(option, enabled: enabled != 0) else {
            callback(nil, 0, context)
            return
        }
        data.withUnsafeBytes { bytes in
            callback(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, context)
        }
    }
}
