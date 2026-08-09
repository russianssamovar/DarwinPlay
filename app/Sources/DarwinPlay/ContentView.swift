import DarwinPlayCore
import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
  @Bindable var model: AppModel

  var body: some View {
    ZStack {
      LauncherBackground()
      VStack(spacing: 0) {
        TopNavigationBar(model: model)
        Rectangle()
          .fill(DarwinPalette.border.opacity(0.55))
          .frame(height: 1)
        detail
          .frame(maxWidth: .infinity, maxHeight: .infinity)
      }
    }
    .fileImporter(
      isPresented: $model.isImporting,
      allowedContentTypes: [.data],
      allowsMultipleSelection: false
    ) { result in
      switch result {
      case .success(let urls):
        if let url = urls.first {
          Task { await model.importGame(url) }
        }
      case .failure(let error):
        model.errorMessage = error.localizedDescription
      }
    }
    .sheet(isPresented: $model.isShowingSettings) {
      SettingsView(model: model)
        .frame(width: 720, height: 650)
    }
    .alert(
      "DarwinPlay",
      isPresented: Binding(
        get: { model.errorMessage != nil },
        set: { if !$0 { model.errorMessage = nil } }
      )
    ) {
      Button("OK") {
        model.errorMessage = nil
      }
    } message: {
      Text(friendlyError(model.errorMessage ?? ""))
    }
  }

  @ViewBuilder
  private var detail: some View {
    switch model.selection {
    case .home, nil:
      DashboardView(model: model)
    case .games, .steamLibrary, .importedLibrary:
      GamesView(model: model)
    case .console:
      ConsoleView(model: model)
    case .steam:
      if let game = model.selectedSteamGame {
        SteamDetailView(model: model, game: game)
      } else {
        GamesView(model: model)
      }
    case .imported:
      if let game = model.selectedGame {
        GameDetailView(model: model, game: game)
      } else {
        GamesView(model: model)
      }
    }
  }

  private func friendlyError(_ message: String) -> String {
    message
  }
}

private struct TopNavigationBar: View {
  @Bindable var model: AppModel

  var body: some View {
    ZStack {
      HStack {
        brand
        Spacer()
        utilities
      }
      HStack(spacing: 28) {
        navButton("Home", selection: .home, active: currentSection == .home)
        navButton("Games", selection: .games, active: currentSection == .games)
        navButton("Console", selection: .console, active: currentSection == .console)
      }
    }
    .padding(.horizontal, 28)
    .frame(height: 76)
    .background(DarwinPalette.background.opacity(0.88))
  }

  private var brand: some View {
    Button {
      model.select(.home)
    } label: {
      HStack(spacing: 10) {
        DarwinMark(size: 36)
        Text("DarwinPlay")
          .font(.system(size: 17, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
      }
    }
    .buttonStyle(.plain)
    .accessibilityLabel("DarwinPlay Home")
  }

  private var utilities: some View {
    Button {
      model.isShowingSettings = true
    } label: {
      Image(systemName: "gearshape")
        .font(.system(size: 15, weight: .semibold))
    }
    .buttonStyle(IconActionButtonStyle())
    .accessibilityLabel("Open Settings")
  }

  private func navButton(
    _ title: String,
    selection: LibrarySelection,
    active: Bool
  ) -> some View {
    Button {
      model.select(selection)
    } label: {
      VStack(spacing: 8) {
        Text(title)
          .font(.system(size: 13, weight: active ? .semibold : .medium))
          .foregroundStyle(active ? DarwinPalette.textPrimary : DarwinPalette.textSecondary)
        Capsule()
          .fill(active ? DarwinPalette.accent : Color.clear)
          .frame(width: active ? 24 : 8, height: 2)
      }
      .frame(width: 72, height: 42)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
  }

  private var currentSection: LibrarySelection {
    switch model.selection {
    case .home, nil:
      .home
    case .console:
      .console
    default:
      .games
    }
  }
}
