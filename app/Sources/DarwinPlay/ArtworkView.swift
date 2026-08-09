import DarwinPlayCore
import SwiftUI

enum SteamArtwork {
  static func posterURL(appID: UInt32) -> URL? {
    URL(string: "https://cdn.cloudflare.steamstatic.com/steam/apps/\(appID)/library_600x900_2x.jpg")
  }

  static func heroURL(appID: UInt32) -> URL? {
    URL(string: "https://cdn.cloudflare.steamstatic.com/steam/apps/\(appID)/library_hero.jpg")
  }
}

struct SteamPosterView: View {
  let game: SteamGame

  var body: some View {
    AsyncImage(
      url: SteamArtwork.posterURL(appID: game.appId),
      transaction: Transaction(animation: .easeOut(duration: 0.2))
    ) { phase in
      switch phase {
      case .success(let image):
        image.resizable().scaledToFill()
      case .empty:
        ZStack {
          ArtworkPlaceholder(seed: UInt64(game.appId), symbol: "gamecontroller.fill")
          ProgressView().controlSize(.small).tint(DarwinPalette.textSecondary)
        }
      case .failure:
        ArtworkPlaceholder(seed: UInt64(game.appId), symbol: "gamecontroller.fill")
      @unknown default:
        ArtworkPlaceholder(seed: UInt64(game.appId), symbol: "gamecontroller.fill")
      }
    }
  }
}

struct SteamHeroArtwork: View {
  let game: SteamGame

  var body: some View {
    AsyncImage(
      url: SteamArtwork.heroURL(appID: game.appId),
      transaction: Transaction(animation: .easeOut(duration: 0.25))
    ) { phase in
      switch phase {
      case .success(let image):
        image.resizable().scaledToFill()
      default:
        ArtworkPlaceholder(seed: UInt64(game.appId), symbol: "gamecontroller.fill")
      }
    }
  }
}

struct SteamGameCard: View {
  @Bindable var model: AppModel
  let game: SteamGame
  @State private var isHovered = false

  var body: some View {
    Button {
      model.select(.steam(game.appId))
    } label: {
      VStack(alignment: .leading, spacing: 10) {
        SteamPosterView(game: game)
          .aspectRatio(2.0 / 3.0, contentMode: .fit)
          .clipShape(RoundedRectangle(cornerRadius: 12))
          .overlay(alignment: .topTrailing) {
            if model.isFavorite(.steam(game)) {
              Image(systemName: "star.fill")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(DarwinPalette.accent)
                .padding(8)
                .background(DarwinPalette.background.opacity(0.76), in: Circle())
                .padding(8)
            }
          }
          .overlay {
            RoundedRectangle(cornerRadius: 12)
              .stroke(
                isHovered ? DarwinPalette.accent.opacity(0.72) : DarwinPalette.border.opacity(0.6),
                lineWidth: isHovered ? 1.5 : 1)
          }

        Text(game.name)
          .font(.system(size: 14, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
          .lineLimit(1)

        HStack(spacing: 7) {
          compatibilityBadge
          Spacer(minLength: 0)
          Text("Installed")
            .font(.system(size: 10.5, weight: .medium))
            .foregroundStyle(DarwinPalette.textTertiary)
        }

        Text(model.lastPlayedText(for: .steam(game)))
          .font(.system(size: 10.5))
          .foregroundStyle(DarwinPalette.textTertiary)
          .lineLimit(1)
      }
      .contentShape(Rectangle())
      .scaleEffect(isHovered ? 1.018 : 1)
      .animation(.easeOut(duration: 0.14), value: isHovered)
    }
    .buttonStyle(.plain)
    .onHover { isHovered = $0 }
    .contextMenu {
      Button(model.isFavorite(.steam(game)) ? "Remove from Favorites" : "Add to Favorites") {
        model.toggleFavorite(.steam(game))
      }
      Button("Play") {
        model.select(.steam(game.appId))
        model.launchSelectedSteamGame()
      }
    }
    .task(id: game.appId) {
      await model.ensureCompatibility(for: game)
    }
    .accessibilityLabel("Open \(game.name)")
  }

  @ViewBuilder
  private var compatibilityBadge: some View {
    if let profile = model.compatibilityProfiles[game.appId] {
      HStack(spacing: 5) {
        Circle()
          .fill(compatibilityColor(profile.compatibility))
          .frame(width: 5, height: 5)
        Text(profile.compatibility.displayName)
          .font(.system(size: 10.5, weight: .medium))
          .foregroundStyle(DarwinPalette.textSecondary)
      }
    } else {
      HStack(spacing: 5) {
        ProgressView().controlSize(.mini)
        Text("Analyzing")
          .font(.system(size: 10.5))
          .foregroundStyle(DarwinPalette.textTertiary)
      }
    }
  }

  private func compatibilityColor(_ level: SteamCompatibilityLevel) -> Color {
    switch level {
    case .promising: DarwinPalette.success
    case .needsComponent: DarwinPalette.warning
    case .fallback: DarwinPalette.info
    case .unsupported: DarwinPalette.danger
    case .unknown: DarwinPalette.textTertiary
    }
  }
}

struct ImportedGameCard: View {
  @Bindable var model: AppModel
  let game: GameRecord
  @State private var isHovered = false

  var body: some View {
    Button {
      model.select(.imported(game.id))
    } label: {
      VStack(alignment: .leading, spacing: 10) {
        ArtworkPlaceholder(seed: seed, symbol: "shippingbox.fill")
          .aspectRatio(2.0 / 3.0, contentMode: .fit)
          .clipShape(RoundedRectangle(cornerRadius: 12))
          .overlay(alignment: .topTrailing) {
            if model.isFavorite(.imported(game)) {
              Image(systemName: "star.fill")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(DarwinPalette.accent)
                .padding(8)
                .background(DarwinPalette.background.opacity(0.76), in: Circle())
                .padding(8)
            }
          }
          .overlay {
            RoundedRectangle(cornerRadius: 12)
              .stroke(
                isHovered ? DarwinPalette.accent.opacity(0.72) : DarwinPalette.border.opacity(0.6),
                lineWidth: isHovered ? 1.5 : 1)
          }

        Text(game.name)
          .font(.system(size: 14, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
          .lineLimit(1)

        HStack(spacing: 6) {
          Circle()
            .fill(
              model.runtimeStatus?.ready == true ? DarwinPalette.success : DarwinPalette.warning
            )
            .frame(width: 5, height: 5)
          Text(model.runtimeStatus?.ready == true ? "Ready" : "Needs Runtime")
            .font(.system(size: 10.5, weight: .medium))
            .foregroundStyle(DarwinPalette.textSecondary)
          Spacer()
          Text("Standalone")
            .font(.system(size: 10.5))
            .foregroundStyle(DarwinPalette.textTertiary)
        }

        Text(model.lastPlayedText(for: .imported(game)))
          .font(.system(size: 10.5))
          .foregroundStyle(DarwinPalette.textTertiary)
      }
      .contentShape(Rectangle())
      .scaleEffect(isHovered ? 1.018 : 1)
      .animation(.easeOut(duration: 0.14), value: isHovered)
    }
    .buttonStyle(.plain)
    .onHover { isHovered = $0 }
    .contextMenu {
      Button(model.isFavorite(.imported(game)) ? "Remove from Favorites" : "Add to Favorites") {
        model.toggleFavorite(.imported(game))
      }
      Button("Play") {
        model.select(.imported(game.id))
        model.launchSelectedGame()
      }
    }
    .accessibilityLabel("Open \(game.name)")
  }

  private var seed: UInt64 {
    game.name.unicodeScalars.reduce(0) { partial, scalar in
      partial &* 31 &+ UInt64(scalar.value)
    }
  }
}
