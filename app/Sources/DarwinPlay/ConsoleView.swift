import SwiftUI

struct ConsoleView: View {
  enum Filter: String, CaseIterable, Identifiable {
    case all = "All"
    case wine = "Wine"
    case steam = "Steam"
    case graphics = "Graphics"
    case errors = "Errors"

    var id: String { rawValue }
  }

  @Bindable var model: AppModel
  @State private var filter: Filter = .all

  var body: some View {
    VStack(alignment: .leading, spacing: 20) {
      LauncherPageHeader(
        title: "Console",
        subtitle: "Wine, Steam and graphics runtime output in one place.",
        actions: AnyView(
          Button {
            model.clearConsole()
          } label: {
            Label("Clear", systemImage: "trash")
          }
          .buttonStyle(SecondaryActionButtonStyle())
        )
      )

      HStack(spacing: 6) {
        ForEach(Filter.allCases) { item in
          Button {
            filter = item
          } label: {
            Text(item.rawValue)
              .font(.system(size: 11.5, weight: item == filter ? .semibold : .medium))
              .foregroundStyle(
                item == filter ? DarwinPalette.textPrimary : DarwinPalette.textSecondary
              )
              .padding(.horizontal, 12)
              .frame(height: 32)
              .background(item == filter ? DarwinPalette.surfaceRaised : Color.clear, in: Capsule())
          }
          .buttonStyle(.plain)
        }
        Spacer()
        RuntimeStateBadge(
          name: "Session",
          value: isRunning ? "Active" : "Idle",
          ready: isRunning
        )
      }

      RuntimeConsoleView(entries: filteredEntries, isRunning: isRunning)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    .padding(.horizontal, 36)
    .padding(.top, 28)
    .padding(.bottom, 32)
    .frame(maxWidth: 1440, maxHeight: .infinity, alignment: .topLeading)
  }

  private var isRunning: Bool {
    model.steamIsRunning || !model.runningGameIDs.isEmpty || model.isManagingWine
  }

  private var filteredEntries: [ConsoleEntry] {
    switch filter {
    case .all:
      model.consoleEntries
    case .wine:
      model.consoleEntries.filter { $0.component == .wine }
    case .steam:
      model.consoleEntries.filter { $0.component == .steam }
    case .graphics:
      model.consoleEntries.filter { $0.component == .graphics }
    case .errors:
      model.consoleEntries.filter { $0.level == .error }
    }
  }
}
