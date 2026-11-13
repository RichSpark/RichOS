import RichFiles
import SwiftUI

@main
struct RichFilesApplication: App {
    @NSApplicationDelegateAdaptor(FolderOpenDelegate.self) private var delegate

    var body: some Scene {
        RichFilesScenes(delegate: delegate)
    }
}
