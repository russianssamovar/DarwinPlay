import DarwinPlayCore
import Foundation
import SwiftUI

struct GameDetailView: View {
  @Bindable var model: AppModel
  let game: GameRecord

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 24) {
        hero
        if let report = model.inspection {
          metadataSection(report)
        }
        prefixSection
      }
      .padding(.horizontal, 36)
      .padding(.top, 24)
      .padding(.bottom, 48)
      .frame(maxWidth: 1180, alignment: .leading)
    }
  }

  private var hero: some View {
    SurfaceCard(padding: 0) {
      HStack(spacing: 0) {
        ArtworkPlaceholder(seed: seed, symbol: "shippingbox.fill")
          .frame(width: 260, height: 360)
          .clipped()

        VStack(alignment: .leading, spacing: 14) {
          Button {
            model.select(.games)
          } label: {
            Label("Games", systemImage: "chevron.left")
              .font(.system(size: 11.5, weight: .semibold))
          }
          .buttonStyle(.plain)
          .foregroundStyle(DarwinPalette.textSecondary)

          Spacer()

          StatusPill("STANDALONE", systemImage: "shippingbox", tone: .info)
          Text(game.name)
            .font(.system(size: 34, weight: .semibold))
            .foregroundStyle(DarwinPalette.textPrimary)
            .lineLimit(2)
          Text(game.executablePath)
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(DarwinPalette.textTertiary)
            .textSelection(.enabled)
            .lineLimit(2)

          HStack(spacing: 9) {
            Button {
              model.launchSelectedGame()
            } label: {
              Label(
                model.runningGameIDs.contains(game.id) ? "Running" : "Play",
                systemImage: "play.fill")
            }
            .buttonStyle(PrimaryActionButtonStyle())
            .disabled(model.runningGameIDs.contains(game.id) || model.wineStatus?.ready != true)

            Button {
              model.toggleFavorite(.imported(game))
            } label: {
              Label(
                model.isFavorite(.imported(game)) ? "Favorited" : "Favorite",
                systemImage: model.isFavorite(.imported(game)) ? "star.fill" : "star"
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

          Text("Last played \(model.lastPlayedText(for: .imported(game)))")
            .font(.system(size: 11.5))
            .foregroundStyle(DarwinPalette.textSecondary)
          Spacer()
        }
        .padding(26)
        .frame(maxWidth: .infinity, alignment: .leading)
      }
      .frame(height: 360)
    }
  }

  private func metadataSection(_ report: PEReport) -> some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading(
        "Executable", subtitle: "Portable Executable metadata detected by the Rust runtime")
      SurfaceCard {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: 18)], spacing: 18) {
          metadata("Architecture", report.architecture)
          metadata("Subsystem", report.subsystem)
          metadata("Entry point", String(format: "0x%X", report.entryPoint))
          metadata("Image base", String(format: "0x%llX", report.imageBase))
          metadata(
            "Graphics",
            report.graphicsApis.isEmpty
              ? "Not detected" : report.graphicsApis.joined(separator: ", "))
        }
      }
    }
  }

  private var prefixSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading("Prefix", subtitle: "This game uses an isolated Wine prefix")
      SurfaceCard {
        HStack(spacing: 10) {
          Button {
            Task { await model.resetSelectedPrefix() }
          } label: {
            Label("Reset Prefix", systemImage: "arrow.counterclockwise")
          }
          .buttonStyle(SecondaryActionButtonStyle())
          .disabled(model.runningGameIDs.contains(game.id))

          Button(role: .destructive) {
            Task { await model.removeSelectedGame() }
          } label: {
            Label("Remove from DarwinPlay", systemImage: "trash")
          }
          .buttonStyle(SecondaryActionButtonStyle())
          .disabled(model.runningGameIDs.contains(game.id))
          Spacer()
        }
      }
    }
  }

  private func metadata(_ title: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(title)
        .font(.system(size: 10.5))
        .foregroundStyle(DarwinPalette.textTertiary)
      Text(value)
        .font(.system(size: 12.5, weight: .semibold, design: .monospaced))
        .foregroundStyle(DarwinPalette.textPrimary)
        .lineLimit(2)
    }
  }

  private var seed: UInt64 {
    game.name.unicodeScalars.reduce(0) { partial, scalar in
      partial &* 31 &+ UInt64(scalar.value)
    }
  }
}
