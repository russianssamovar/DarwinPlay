import DarwinPlayCore
import SwiftUI

struct GamesView: View {
  enum Filter: String, CaseIterable, Identifiable {
    case steam = "Steam"
    case imported = "Imported"

    var id: String { rawValue }
  }

  @Bindable var model: AppModel
  @State private var filter: Filter = .steam

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 24) {
        header
        filterBar
        content
      }
      .padding(.horizontal, 36)
      .padding(.top, 28)
      .padding(.bottom, 48)
      .frame(maxWidth: 1440, alignment: .leading)
    }
  }

  private var header: some View {
    LauncherPageHeader(
      title: "Games",
      subtitle: "Your Windows library, with compatibility handled per game.",
      actions: AnyView(
        HStack(spacing: 8) {
          if filter == .steam, model.steamStatus?.installed == true {
            Button {
              Task { await model.refreshSteam() }
            } label: {
              Label("Refresh", systemImage: "arrow.clockwise")
            }
            .buttonStyle(SecondaryActionButtonStyle())

            Button {
              if model.steamUiRestartRequired {
                model.restartSteamUI()
              } else {
                model.openSteam()
              }
            } label: {
              Label(
                steamButtonTitle,
                systemImage: steamButtonIcon
              )
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(model.steamIsRunning && !model.steamUiRestartRequired)
          }

          if filter == .imported {
            Button {
              model.isImporting = true
            } label: {
              Label("Import EXE", systemImage: "plus")
            }
            .buttonStyle(SecondaryActionButtonStyle())
          }
        }
      )
    )
  }

  private var filterBar: some View {
    HStack(spacing: 6) {
      ForEach(Filter.allCases) { item in
        Button {
          filter = item
        } label: {
          HStack(spacing: 7) {
            Text(item.rawValue)
            Text(item == .steam ? "\(model.steamGames.count)" : "\(model.games.count)")
              .foregroundStyle(DarwinPalette.textTertiary)
          }
          .font(.system(size: 12.5, weight: item == filter ? .semibold : .medium))
          .foregroundStyle(item == filter ? DarwinPalette.textPrimary : DarwinPalette.textSecondary)
          .padding(.horizontal, 13)
          .frame(height: 34)
          .background(item == filter ? DarwinPalette.surfaceRaised : Color.clear, in: Capsule())
        }
        .buttonStyle(.plain)
      }
      Spacer()
    }
  }

  @ViewBuilder
  private var content: some View {
    switch filter {
    case .steam:
      steamContent
    case .imported:
      importedContent
    }
  }

  @ViewBuilder
  private var steamContent: some View {
    if model.steamStatus?.installed != true {
      setupRequired(
        title: "Steam is not set up yet",
        message:
          "Complete the one-time setup from Home. After that, Steam management stays in Settings."
      )
    } else if model.steamGames.isEmpty {
      SurfaceCard(padding: 24) {
        HStack(spacing: 18) {
          Image(systemName: "rectangle.stack.badge.plus")
            .font(.system(size: 28, weight: .medium))
            .foregroundStyle(DarwinPalette.accentSoft)
          VStack(alignment: .leading, spacing: 5) {
            Text("No installed Steam games")
              .font(.system(size: 18, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            Text("Open Windows Steam, install a game, then refresh this page.")
              .font(.system(size: 12.5))
              .foregroundStyle(DarwinPalette.textSecondary)
          }
          Spacer()
          Button {
            if model.steamUiRestartRequired {
              model.restartSteamUI()
            } else {
              model.openSteam()
            }
          } label: {
            Label(steamButtonTitle, systemImage: steamButtonIcon)
          }
          .buttonStyle(PrimaryActionButtonStyle())
          .disabled(model.steamIsRunning && !model.steamUiRestartRequired)
        }
      }
    } else {
      LazyVGrid(
        columns: [GridItem(.adaptive(minimum: 172, maximum: 205), spacing: 20)],
        spacing: 24
      ) {
        ForEach(model.steamGames) { game in
          SteamGameCard(model: model, game: game)
        }
      }
    }
  }

  private var steamButtonTitle: String {
    if model.steamUiRestartRequired {
      return "Restart Steam UI"
    }
    return model.steamIsRunning ? "Steam Running" : "Open Steam"
  }

  private var steamButtonIcon: String {
    if model.steamUiRestartRequired {
      return "arrow.clockwise"
    }
    return model.steamIsRunning ? "checkmark.circle.fill" : "play.rectangle"
  }

  @ViewBuilder
  private var importedContent: some View {
    if model.games.isEmpty {
      SurfaceCard(padding: 24) {
        HStack(spacing: 18) {
          Image(systemName: "shippingbox")
            .font(.system(size: 28, weight: .medium))
            .foregroundStyle(DarwinPalette.accentSoft)
          VStack(alignment: .leading, spacing: 5) {
            Text("No standalone games")
              .font(.system(size: 18, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            Text("Import a Windows executable to launch it in its own Wine prefix.")
              .font(.system(size: 12.5))
              .foregroundStyle(DarwinPalette.textSecondary)
          }
          Spacer()
          Button {
            model.isImporting = true
          } label: {
            Label("Import EXE", systemImage: "plus")
          }
          .buttonStyle(PrimaryActionButtonStyle())
        }
      }
    } else {
      LazyVGrid(
        columns: [GridItem(.adaptive(minimum: 172, maximum: 205), spacing: 20)],
        spacing: 24
      ) {
        ForEach(model.games) { game in
          ImportedGameCard(model: model, game: game)
        }
      }
    }
  }

  private func setupRequired(title: String, message: String) -> some View {
    SurfaceCard(padding: 24) {
      HStack(spacing: 18) {
        DarwinMark(size: 48)
        VStack(alignment: .leading, spacing: 5) {
          Text(title)
            .font(.system(size: 18, weight: .semibold))
            .foregroundStyle(DarwinPalette.textPrimary)
          Text(message)
            .font(.system(size: 12.5))
            .foregroundStyle(DarwinPalette.textSecondary)
        }
        Spacer()
        Button {
          model.select(.home)
        } label: {
          Text("Go to Home")
        }
        .buttonStyle(SecondaryActionButtonStyle())
      }
    }
  }
}
