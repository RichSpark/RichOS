import RichFiles
import SwiftUI

@main
enum RichFilesEntry {
    @MainActor static func main() {
        if !WindowGestureService.runCommandIfRequested() {
            RichFilesApplication.main()
        }
    }
}

struct RichFilesApplication: App {
    @NSApplicationDelegateAdaptor(FolderOpenDelegate.self) private var delegate

    var body: some Scene {
        RichFilesScenes(delegate: delegate)
    }
}
