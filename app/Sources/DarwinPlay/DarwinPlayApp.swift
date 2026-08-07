import DarwinPlayCore
import SwiftUI

@main
struct DarwinPlayApp: App {
  @State private var model = AppModel()

  var body: some Scene {
    WindowGroup {
      ContentView(model: model)
        .frame(minWidth: 1100, minHeight: 720)
        .preferredColorScheme(.dark)
    }
    .windowStyle(.hiddenTitleBar)
  }
}
