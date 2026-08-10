import DarwinPlayCore
import SwiftUI

struct DashboardView: View {
  @Bindable var model: AppModel

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 30) {
        if needsSetup {
          setupSection
        } else {
          hero
          recentSection
          favoritesSection
          runtimeSection
        }
      }
      .padding(.horizontal, 36)
      .padding(.top, 28)
      .padding(.bottom, 48)
      .frame(maxWidth: 1440, alignment: .leading)
    }
  }

  private var needsSetup: Bool {
    model.runtimeStatus?.ready != true
      || model.steamStatus?.installed != true
  }

  @ViewBuilder
  private var setupSection: some View {
    VStack(alignment: .leading, spacing: 22) {
      VStack(alignment: .leading, spacing: 6) {
        Text("Set up DarwinPlay")
          .font(.system(size: 32, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
        Text("Two steps, then the launcher gets out of your way.")
          .font(.system(size: 14))
          .foregroundStyle(DarwinPalette.textSecondary)
      }

      HStack(alignment: .top, spacing: 18) {
        setupCard(
          title: "DarwinWine Runtime",
          subtitle: "DarwinWine cx26.3-dp5 or newer",
          step: 1,
          ready: model.runtimeStatus?.ready == true,
          enabled: true,
          actionTitle: runtimeActionTitle,
          symbol: "wineglass.fill"
        ) {
          if model.runtimeStatus?.ready == true {
            model.isShowingSettings = true
          } else {
            Task { await model.installLatestDarwinWine() }
          }
        }

        setupCard(
          title: "Steam for Windows",
          subtitle: "Your Windows game library",
          step: 2,
          ready: model.runtimeStatus?.ready == true && model.steamStatus?.installed == true,
          enabled: model.runtimeStatus?.ready == true,
          actionTitle: model.steamStatus?.installed == true
            ? "Manage in Settings" : "Install Steam",
          symbol: "gamecontroller.fill"
        ) {
          if model.steamStatus?.installed == true {
            model.isShowingSettings = true
          } else {
            Task { await model.installSteam() }
          }
        }
      }

      if model.runtimeStatus?.installed == true, model.runtimeStatus?.ready != true {
        darwinWineAttentionPanel
      }

      HStack(spacing: 18) {
        statusLine(
          title: "Runtime",
          detail: runtimeStatusDetail,
          ready: model.runtimeStatus?.ready == true
        )
        statusLine(
          title: "Steam",
          detail: steamStatusDetail,
          ready: model.runtimeStatus?.ready == true && model.steamStatus?.installed == true
        )
        Spacer()
      }
    }
    .frame(maxWidth: 1060, alignment: .leading)
  }

  private var runtimeActionTitle: String {
    if model.runtimeStatus?.ready == true { return "Manage in Settings" }
    if model.isManagingDarwinWine { return "Installing Runtime…" }
    if model.runtimeStatus?.installed == true { return "Reinstall Runtime" }
    return "Download & Install"
  }

  private func setupCard(
    title: String,
    subtitle: String,
    step: Int,
    ready: Bool,
    enabled: Bool,
    actionTitle: String,
    symbol: String,
    action: @escaping () -> Void
  ) -> some View {
    VStack(alignment: .leading, spacing: 0) {
      ZStack {
        DarwinPalette.surfaceRaised
        VStack(spacing: 14) {
          if step == 1 {
            DarwinMark(size: 88)
          } else {
            Image(systemName: symbol)
              .font(.system(size: 54, weight: .medium))
              .foregroundStyle(enabled ? DarwinPalette.accentSoft : DarwinPalette.textTertiary)
          }
          Text("STEP \(step)")
            .font(.system(size: 9, weight: .bold))
            .tracking(1.4)
            .foregroundStyle(DarwinPalette.textTertiary)
        }
      }
      .frame(height: 250)

      VStack(alignment: .leading, spacing: 10) {
        HStack {
          VStack(alignment: .leading, spacing: 4) {
            Text(title)
              .font(.system(size: 18, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            Text(subtitle)
              .font(.system(size: 12))
              .foregroundStyle(DarwinPalette.textSecondary)
          }
          Spacer()
          if ready {
            Image(systemName: "checkmark.circle.fill")
              .foregroundStyle(DarwinPalette.accent)
          }
        }

        Button(action: action) {
          HStack(spacing: 8) {
            if (step == 1 && model.isManagingDarwinWine)
              || (step == 2 && model.isInstallingSteam)
            {
              ProgressView().controlSize(.small)
            }
            Text(actionTitle)
          }
          .frame(maxWidth: .infinity)
        }
        .buttonStyle(ActionButtonStyle(emphasized: !ready))
        .disabled(
          !enabled || model.isInstallingSteam || model.isManagingDarwinWine
        )

        if step == 1, let progress = model.darwinWineProgress {
          OperationProgressView(state: progress)
        } else if step == 2, let progress = model.steamInstallProgress {
          OperationProgressView(state: progress)
        }
      }
      .padding(16)
    }
    .frame(width: 300)
    .background(DarwinPalette.backgroundElevated, in: RoundedRectangle(cornerRadius: 16))
    .clipShape(RoundedRectangle(cornerRadius: 16))
    .overlay {
      RoundedRectangle(cornerRadius: 16)
        .stroke(ready ? DarwinPalette.accent.opacity(0.35) : DarwinPalette.border, lineWidth: 1)
    }
    .opacity(enabled || ready ? 1 : 0.48)
  }

  private var darwinWineAttentionPanel: some View {
    SurfaceCard(padding: 18) {
      HStack(alignment: .top, spacing: 16) {
        Image(systemName: "wrench.and.screwdriver.fill")
          .font(.system(size: 20, weight: .medium))
          .foregroundStyle(DarwinPalette.warning)
          .frame(width: 32)

        VStack(alignment: .leading, spacing: 7) {
          Text("DarwinWine needs reinstall")
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(DarwinPalette.textPrimary)
          Text(
            model.runtimeStatus?.probeError
              ?? "The installed DarwinWine runtime did not pass validation."
          )
          .font(.system(size: 11.5, design: .monospaced))
          .foregroundStyle(DarwinPalette.textSecondary)
          .textSelection(.enabled)
          .lineLimit(4)
        }

        Spacer(minLength: 18)

        Button {
          Task { await model.installLatestDarwinWine() }
        } label: {
          Text(model.isManagingDarwinWine ? "Installing…" : "Reinstall Runtime")
        }
        .buttonStyle(PrimaryActionButtonStyle())
        .disabled(model.isManagingDarwinWine || model.steamIsRunning)
      }
    }
  }

  private var hero: some View {
    Group {
      if let item = model.latestItem {
        HomeHero(model: model, item: item)
      } else {
        SurfaceCard(padding: 28) {
          HStack(spacing: 22) {
            DarwinMark(size: 64)
            VStack(alignment: .leading, spacing: 7) {
              Text("Ready when you are")
                .font(.system(size: 28, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
              Text("Launch a game from Games and it will appear here next time.")
                .font(.system(size: 13))
                .foregroundStyle(DarwinPalette.textSecondary)
            }
            Spacer()
            Button {
              model.select(.games)
            } label: {
              Label("Browse Games", systemImage: "square.grid.2x2")
            }
            .buttonStyle(PrimaryActionButtonStyle())
          }
        }
      }
    }
  }

  private var recentSection: some View {
    libraryRow(
      title: "Recently played",
      items: Array(model.recentItems.dropFirst().prefix(7)),
      emptyText: "Your recently launched games will appear here."
    )
  }

  private var favoritesSection: some View {
    libraryRow(
      title: "Favorites",
      items: Array(model.favoriteItems.prefix(8)),
      emptyText: "Favorite a game from its detail page or card menu."
    )
  }

  private func libraryRow(title: String, items: [PlayableItem], emptyText: String) -> some View {
    VStack(alignment: .leading, spacing: 14) {
      SectionHeading(title)
      if items.isEmpty {
        Text(emptyText)
          .font(.system(size: 12.5))
          .foregroundStyle(DarwinPalette.textTertiary)
          .padding(.vertical, 10)
      } else {
        ScrollView(.horizontal, showsIndicators: false) {
          LazyHStack(alignment: .top, spacing: 16) {
            ForEach(items) { item in
              compactCard(item)
                .frame(width: 174)
            }
          }
          .padding(.vertical, 2)
        }
      }
    }
  }

  @ViewBuilder
  private func compactCard(_ item: PlayableItem) -> some View {
    switch item {
    case .steam(let game):
      SteamGameCard(model: model, game: game)
    case .imported(let game):
      ImportedGameCard(model: model, game: game)
    }
  }

  private var runtimeSection: some View {
    VStack(alignment: .leading, spacing: 14) {
      SectionHeading("Runtime")
      HStack(spacing: 12) {
        statusLine(
          title: "Runtime",
          detail: runtimeStatusDetail,
          ready: model.runtimeStatus?.ready == true
        )
        statusLine(
          title: "Steam",
          detail: steamStatusDetail,
          ready: model.runtimeStatus?.ready == true && model.steamStatus?.installed == true
        )
        Spacer()
      }
    }
  }

  private var steamStatusDetail: String {
    if model.runtimeStatus?.ready != true {
      return model.steamStatus?.installed == true ? "Waiting for runtime" : "Not installed"
    }
    if model.steamIsRunning {
      return "Running"
    }
    return model.steamStatus?.installed == true ? "Ready" : "Not installed"
  }

  private var runtimeStatusDetail: String {
    if model.runtimeStatus?.ready == true {
      return model.runtimeStatus?.darwinWineVersion ?? model.runtimeStatus?.wineVersion ?? "Ready"
    }
    if model.runtimeStatus?.installed == true { return "Needs reinstall" }
    return "Not installed"
  }

  private func statusLine(title: String, detail: String, ready: Bool) -> some View {
    HStack(spacing: 8) {
      Circle()
        .fill(ready ? DarwinPalette.accent : DarwinPalette.warning)
        .frame(width: 6, height: 6)
      Text(title)
        .font(.system(size: 11.5, weight: .semibold))
        .foregroundStyle(DarwinPalette.textPrimary)
      Text(detail)
        .font(.system(size: 11.5))
        .foregroundStyle(DarwinPalette.textSecondary)
    }
    .padding(.horizontal, 12)
    .frame(height: 34)
    .background(DarwinPalette.surface.opacity(0.65), in: Capsule())
  }
}

private struct HomeHero: View {
  @Bindable var model: AppModel
  let item: PlayableItem

  var body: some View {
    ZStack(alignment: .bottomLeading) {
      heroArtwork
        .frame(height: 390)
        .clipped()

      LinearGradient(
        colors: [
          .clear, DarwinPalette.background.opacity(0.38), DarwinPalette.background.opacity(0.96),
        ],
        startPoint: .top,
        endPoint: .bottom
      )

      LinearGradient(
        colors: [DarwinPalette.background.opacity(0.92), .clear],
        startPoint: .leading,
        endPoint: .trailing
      )

      VStack(alignment: .leading, spacing: 12) {
        Text("CONTINUE")
          .font(.system(size: 10, weight: .bold))
          .tracking(1.6)
          .foregroundStyle(DarwinPalette.accentSoft)
        Text(item.name)
          .font(.system(size: 34, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
          .lineLimit(2)
          .frame(maxWidth: 560, alignment: .leading)

        HStack(spacing: 9) {
          Button {
            model.launch(item)
          } label: {
            Label("Play", systemImage: "play.fill")
          }
          .buttonStyle(PrimaryActionButtonStyle())

          Button {
            model.select(item)
          } label: {
            Text("Details")
          }
          .buttonStyle(SecondaryActionButtonStyle())

          Button {
            model.toggleFavorite(item)
          } label: {
            Image(systemName: model.isFavorite(item) ? "star.fill" : "star")
          }
          .buttonStyle(IconActionButtonStyle())
          .accessibilityLabel(model.isFavorite(item) ? "Remove from Favorites" : "Add to Favorites")
        }

        Text("Last played \(model.lastPlayedText(for: item))")
          .font(.system(size: 11.5))
          .foregroundStyle(DarwinPalette.textSecondary)
      }
      .padding(28)
    }
    .background(DarwinPalette.surface, in: RoundedRectangle(cornerRadius: 20))
    .clipShape(RoundedRectangle(cornerRadius: 20))
    .overlay {
      RoundedRectangle(cornerRadius: 20)
        .stroke(DarwinPalette.border.opacity(0.75), lineWidth: 1)
    }
    .task {
      if case .steam(let game) = item {
        await model.ensureCompatibility(for: game)
      }
    }
  }

  @ViewBuilder
  private var heroArtwork: some View {
    switch item {
    case .steam(let game):
      SteamHeroArtwork(game: game)
    case .imported(let game):
      ArtworkPlaceholder(seed: seed(game.name), symbol: "shippingbox.fill")
    }
  }

  private func seed(_ name: String) -> UInt64 {
    name.unicodeScalars.reduce(0) { partial, scalar in
      partial &* 31 &+ UInt64(scalar.value)
    }
  }
}
