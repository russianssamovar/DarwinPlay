import DarwinPlayCore
import Foundation
import SwiftUI

struct SteamDetailView: View {
  @Bindable var model: AppModel
  let game: SteamGame
  @State private var backendOverride: SteamBackendOverride = .inherit
  @State private var selectedExecutable = ""
  @State private var launchArgumentsText = ""

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 24) {
        hero
        compatibilitySection
        HStack(alignment: .top, spacing: 18) {
          profileSection
            .frame(maxWidth: .infinity, alignment: .top)
          installationSection
            .frame(width: 320, alignment: .top)
        }
        candidatesSection
      }
      .padding(.horizontal, 36)
      .padding(.top, 24)
      .padding(.bottom, 48)
      .frame(maxWidth: 1320, alignment: .leading)
    }
    .task(id: game.appId) {
      await model.refreshSteamProfile()
      syncProfile()
    }
    .onChange(of: model.steamProfile) { _, _ in
      syncProfile()
    }
  }

  private var hero: some View {
    ZStack(alignment: .bottomLeading) {
      SteamHeroArtwork(game: game)
        .frame(height: 360)
        .clipped()

      LinearGradient(
        colors: [
          .clear, DarwinPalette.background.opacity(0.45), DarwinPalette.background.opacity(0.97),
        ],
        startPoint: .top,
        endPoint: .bottom
      )
      LinearGradient(
        colors: [DarwinPalette.background.opacity(0.90), .clear],
        startPoint: .leading,
        endPoint: .trailing
      )

      VStack(alignment: .leading, spacing: 13) {
        Button {
          model.select(.games)
        } label: {
          Label("Games", systemImage: "chevron.left")
            .font(.system(size: 11.5, weight: .semibold))
        }
        .buttonStyle(.plain)
        .foregroundStyle(DarwinPalette.textSecondary)

        Text(game.name)
          .font(.system(size: 36, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
          .lineLimit(2)
          .frame(maxWidth: 650, alignment: .leading)

        if let profile = currentProfile {
          HStack(spacing: 8) {
            StatusPill(
              profile.compatibility.displayName,
              systemImage: compatibilityIcon(profile.compatibility),
              tone: compatibilityTone(profile.compatibility)
            )
            StatusPill(
              profile.effectiveBackend.displayName,
              systemImage: "rectangle.3.group",
              tone: .info
            )
          }
        }

        HStack(spacing: 9) {
          Button {
            model.launchSelectedSteamGame()
          } label: {
            Label(
              model.steamLaunchingAppID == game.appId ? "Launching…" : "Play",
              systemImage: "play.fill")
          }
          .buttonStyle(PrimaryActionButtonStyle())
          .disabled(model.steamLaunchingAppID == game.appId)

          Button {
            model.toggleFavorite(.steam(game))
          } label: {
            Label(
              model.isFavorite(.steam(game)) ? "Favorited" : "Favorite",
              systemImage: model.isFavorite(.steam(game)) ? "star.fill" : "star"
            )
          }
          .buttonStyle(SecondaryActionButtonStyle())

          Button {
            model.select(.console)
          } label: {
            Label("Console", systemImage: "terminal")
          }
          .buttonStyle(SecondaryActionButtonStyle())
        }

        Text("Last played \(model.lastPlayedText(for: .steam(game)))")
          .font(.system(size: 11.5))
          .foregroundStyle(DarwinPalette.textSecondary)
      }
      .padding(28)
    }
    .background(DarwinPalette.surface, in: RoundedRectangle(cornerRadius: 20))
    .clipShape(RoundedRectangle(cornerRadius: 20))
    .overlay {
      RoundedRectangle(cornerRadius: 20)
        .stroke(DarwinPalette.border.opacity(0.7), lineWidth: 1)
    }
  }

  @ViewBuilder
  private var compatibilitySection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading("Compatibility", subtitle: "Detected from the installed Windows executables")
      if let profile = currentProfile {
        SurfaceCard {
          HStack(alignment: .top, spacing: 28) {
            VStack(alignment: .leading, spacing: 7) {
              Text(profile.compatibility.displayName)
                .font(.system(size: 21, weight: .semibold))
                .foregroundStyle(compatibilityColor(profile.compatibility))
              Text(profile.recommendedExecutable ?? "No executable recommendation")
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(DarwinPalette.textSecondary)
                .textSelection(.enabled)
            }

            Spacer()

            VStack(alignment: .leading, spacing: 5) {
              Text("Recommended backend")
                .font(.system(size: 10.5))
                .foregroundStyle(DarwinPalette.textTertiary)
              Text(profile.recommendedBackend?.displayName ?? "None")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
            }

            VStack(alignment: .leading, spacing: 5) {
              Text("Effective backend")
                .font(.system(size: 10.5))
                .foregroundStyle(DarwinPalette.textTertiary)
              Text(profile.effectiveBackend.displayName)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
            }
          }

          if !profile.reasons.isEmpty {
            Divider().overlay(DarwinPalette.border).padding(.vertical, 12)
            VStack(alignment: .leading, spacing: 6) {
              ForEach(profile.reasons, id: \.self) { reason in
                HStack(alignment: .top, spacing: 8) {
                  Circle()
                    .fill(DarwinPalette.textTertiary)
                    .frame(width: 4, height: 4)
                    .padding(.top, 6)
                  Text(reason)
                    .font(.system(size: 11.5))
                    .foregroundStyle(DarwinPalette.textSecondary)
                }
              }
            }
          }
        }
      } else {
        SurfaceCard {
          HStack(spacing: 10) {
            ProgressView().controlSize(.small)
            Text("Analyzing installed executables…")
              .font(.system(size: 12.5))
              .foregroundStyle(DarwinPalette.textSecondary)
          }
        }
      }
    }
  }

  private var profileSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading("Game profile", subtitle: "Overrides apply only to this Steam AppID")
      SurfaceCard {
        VStack(alignment: .leading, spacing: 16) {
          settingRow("Graphics backend") {
            Picker("Graphics backend", selection: $backendOverride) {
              ForEach(SteamBackendOverride.allCases) { backend in
                Text(backend.displayName).tag(backend)
              }
            }
            .labelsHidden()
            .frame(width: 210)
          }

          Divider().overlay(DarwinPalette.border)

          VStack(alignment: .leading, spacing: 7) {
            Text("Analysis target")
              .font(.system(size: 12, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            Picker("Analysis target", selection: $selectedExecutable) {
              Text("Automatic").tag("")
              if let profile = currentProfile {
                ForEach(profile.candidates) { candidate in
                  Text(candidate.relativePath).tag(candidate.relativePath)
                }
              }
            }
            .labelsHidden()
            .frame(maxWidth: .infinity)
            Text(
              "Steam still launches the game by AppID. This executable is used for compatibility analysis."
            )
            .font(.system(size: 10.5))
            .foregroundStyle(DarwinPalette.textTertiary)
          }

          Divider().overlay(DarwinPalette.border)

          VStack(alignment: .leading, spacing: 7) {
            Text("Launch arguments")
              .font(.system(size: 12, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            TextEditor(text: $launchArgumentsText)
              .font(.system(size: 11.5, design: .monospaced))
              .scrollContentBackground(.hidden)
              .padding(8)
              .frame(minHeight: 78)
              .background(DarwinPalette.console, in: RoundedRectangle(cornerRadius: 9))
              .overlay {
                RoundedRectangle(cornerRadius: 9).stroke(DarwinPalette.border, lineWidth: 1)
              }
            Text("One argument per line")
              .font(.system(size: 10.5))
              .foregroundStyle(DarwinPalette.textTertiary)
          }

          HStack(spacing: 8) {
            Button {
              Task {
                await model.saveSteamProfile(
                  backend: backendOverride,
                  executable: selectedExecutable.isEmpty ? nil : selectedExecutable,
                  launchArguments: launchArguments
                )
              }
            } label: {
              Text("Save Profile")
            }
            .buttonStyle(PrimaryActionButtonStyle())

            Button {
              Task { await model.resetSteamProfile() }
            } label: {
              Text("Reset")
            }
            .buttonStyle(SecondaryActionButtonStyle())
          }
        }
      }
    }
  }

  private var installationSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading("Installation")
      SurfaceCard {
        VStack(alignment: .leading, spacing: 14) {
          metadata("Steam AppID", String(game.appId))
          metadata("State", "Installed")
          metadata(
            "Size",
            ByteCountFormatter.string(
              fromByteCount: Int64(clamping: game.sizeOnDisk), countStyle: .file)
          )
          Divider().overlay(DarwinPalette.border)
          Text(game.installPath)
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(DarwinPalette.textTertiary)
            .textSelection(.enabled)
        }
      }
    }
  }

  @ViewBuilder
  private var candidatesSection: some View {
    if let profile = currentProfile, !profile.candidates.isEmpty {
      VStack(alignment: .leading, spacing: 12) {
        SectionHeading(
          "Executable analysis", subtitle: "Candidates ranked by likely game entry point")
        SurfaceCard(padding: 0) {
          VStack(spacing: 0) {
            ForEach(Array(profile.candidates.prefix(12).enumerated()), id: \.element.id) {
              index, candidate in
              candidateRow(candidate)
              if index < min(profile.candidates.count, 12) - 1 {
                Divider().overlay(DarwinPalette.border.opacity(0.65))
              }
            }
          }
        }
      }
    }
  }

  private func candidateRow(_ candidate: SteamExecutableCandidate) -> some View {
    HStack(spacing: 14) {
      VStack(alignment: .leading, spacing: 4) {
        Text(candidate.relativePath)
          .font(.system(size: 11.5, weight: .medium, design: .monospaced))
          .foregroundStyle(DarwinPalette.textPrimary)
          .lineLimit(1)
        Text(
          candidate.graphicsApis.isEmpty
            ? candidate.architecture : candidate.graphicsApis.joined(separator: " · ")
        )
        .font(.system(size: 10.5))
        .foregroundStyle(DarwinPalette.textTertiary)
      }
      Spacer()
      Text(candidate.kind.rawValue.capitalized)
        .font(.system(size: 10.5))
        .foregroundStyle(DarwinPalette.textSecondary)
      Text("\(candidate.score)")
        .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
        .foregroundStyle(DarwinPalette.textTertiary)
        .frame(width: 44, alignment: .trailing)
    }
    .padding(.horizontal, 16)
    .frame(minHeight: 52)
  }

  private func settingRow<Content: View>(_ title: String, @ViewBuilder content: () -> Content)
    -> some View
  {
    HStack {
      Text(title)
        .font(.system(size: 12, weight: .semibold))
        .foregroundStyle(DarwinPalette.textPrimary)
      Spacer()
      content()
    }
  }

  private func metadata(_ title: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: 3) {
      Text(title)
        .font(.system(size: 10.5))
        .foregroundStyle(DarwinPalette.textTertiary)
      Text(value)
        .font(.system(size: 12.5, weight: .semibold))
        .foregroundStyle(DarwinPalette.textPrimary)
    }
  }

  private var currentProfile: SteamCompatibilityProfile? {
    guard let profile = model.steamProfile, profile.appId == game.appId else { return nil }
    return profile
  }

  private var launchArguments: [String] {
    launchArgumentsText
      .split(whereSeparator: \.isNewline)
      .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
  }

  private func syncProfile() {
    guard let profile = currentProfile else { return }
    backendOverride = profile.backendOverride
    selectedExecutable = profile.selectedExecutable ?? ""
    launchArgumentsText = profile.launchArguments.joined(separator: "\n")
  }

  private func compatibilityTone(_ level: SteamCompatibilityLevel) -> StatusPill.Tone {
    switch level {
    case .promising: .success
    case .needsComponent: .warning
    case .fallback: .info
    case .unsupported: .danger
    case .unknown: .neutral
    }
  }

  private func compatibilityIcon(_ level: SteamCompatibilityLevel) -> String {
    switch level {
    case .promising: "checkmark.circle.fill"
    case .needsComponent: "wrench.and.screwdriver.fill"
    case .fallback: "arrow.triangle.branch"
    case .unsupported: "xmark.circle.fill"
    case .unknown: "questionmark.circle"
    }
  }

  private func compatibilityColor(_ level: SteamCompatibilityLevel) -> Color {
    switch level {
    case .promising: DarwinPalette.success
    case .needsComponent: DarwinPalette.warning
    case .fallback: DarwinPalette.info
    case .unsupported: DarwinPalette.danger
    case .unknown: DarwinPalette.textSecondary
    }
  }
}
