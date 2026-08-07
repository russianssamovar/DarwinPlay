import AppKit
import DarwinPlayCore
import SwiftUI

struct SettingsView: View {
  @Environment(\.dismiss) private var dismiss
  @Bindable var model: AppModel
  @State private var winePath: String
  @State private var graphicsBackend: GraphicsBackendPreference
  @State private var dxmtMode: DxmtMode = .builtin

  init(model: AppModel) {
    self.model = model
    _winePath = State(initialValue: model.settings.winePath)
    _graphicsBackend = State(initialValue: model.settings.graphicsBackend)
  }

  var body: some View {
    ZStack {
      LauncherBackground()
      VStack(spacing: 0) {
        header
        Rectangle().fill(DarwinPalette.border.opacity(0.65)).frame(height: 1)
        ScrollView {
          VStack(alignment: .leading, spacing: 24) {
            runtimeSection
            graphicsSection
            advancedSection
          }
          .padding(24)
        }
        Rectangle().fill(DarwinPalette.border.opacity(0.65)).frame(height: 1)
        footer
      }
    }
    .preferredColorScheme(.dark)
  }

  private var header: some View {
    HStack(spacing: 12) {
      DarwinMark(size: 38)
      VStack(alignment: .leading, spacing: 2) {
        Text("Settings")
          .font(.system(size: 22, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
        Text("Runtime management and compatibility defaults")
          .font(.system(size: 11.5))
          .foregroundStyle(DarwinPalette.textSecondary)
      }
      Spacer()
      Button {
        dismiss()
      } label: {
        Image(systemName: "xmark")
          .font(.system(size: 12, weight: .bold))
      }
      .buttonStyle(IconActionButtonStyle())
      .accessibilityLabel("Close Settings")
    }
    .padding(20)
  }

  private var runtimeSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading("Runtimes", subtitle: "Install once on Home, manage them here afterwards")
      VStack(spacing: 10) {
        wineRuntimeCard
        steamRuntimeCard
      }
    }
  }

  private var wineStatusLabel: String {
    if model.wineStatus?.ready == true { return "READY" }
    if model.wineStatus?.installed == true { return "APPROVAL REQUIRED" }
    return "NOT INSTALLED"
  }

  private var wineRuntimeCard: some View {
    SurfaceCard {
      HStack(alignment: .top, spacing: 14) {
        DarwinMark(size: 42)
        VStack(alignment: .leading, spacing: 5) {
          HStack(spacing: 8) {
            Text("Wine")
              .font(.system(size: 15, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            StatusPill(
              wineStatusLabel,
              tone: model.wineStatus?.ready == true ? .success : .warning
            )
          }
          Text(model.wineStatus?.wineVersion ?? "Windows compatibility runtime")
            .font(.system(size: 11.5))
            .foregroundStyle(DarwinPalette.textSecondary)
          if let path = model.wineStatus?.winePath {
            Text(path)
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(DarwinPalette.textTertiary)
              .textSelection(.enabled)
              .lineLimit(2)
          }
          if model.wineStatus?.ready != true, let error = model.wineStatus?.probeError {
            Text(error)
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(DarwinPalette.warning)
              .textSelection(.enabled)
              .lineLimit(3)
          }
        }
        Spacer(minLength: 12)

        if model.wineStatus?.installed == true, model.wineStatus?.ready != true {
          HStack(spacing: 8) {
            Button("Open Wine") {
              model.openWineApplication()
            }
            .buttonStyle(SecondaryActionButtonStyle())

            Button("Privacy & Security") {
              model.openPrivacyAndSecurity()
            }
            .buttonStyle(SecondaryActionButtonStyle())

            Button("Try Again") {
              model.retryWineProbe()
            }
            .buttonStyle(PrimaryActionButtonStyle())
          }
        } else if model.wineStatus?.installed == true, model.wineStatus?.managedByHomebrew == true {
          HStack(spacing: 8) {
            Button {
              model.reinstallWine()
            } label: {
              Text(model.isManagingWine ? model.wineManagementTitle : "Reinstall")
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(model.isManagingWine)

            Button(role: .destructive) {
              model.removeWine()
            } label: {
              Text("Remove")
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(
              model.isManagingWine || model.steamIsRunning || !model.runningGameIDs.isEmpty)
          }
        } else if model.wineStatus?.installed != true {
          Text("Use Home to complete setup")
            .font(.system(size: 11))
            .foregroundStyle(DarwinPalette.textTertiary)
        }
      }
    }
  }

  private var steamRuntimeCard: some View {
    SurfaceCard {
      HStack(alignment: .top, spacing: 14) {
        Image(systemName: "gamecontroller.fill")
          .font(.system(size: 20, weight: .medium))
          .foregroundStyle(DarwinPalette.accentSoft)
          .frame(width: 42, height: 42)
          .background(DarwinPalette.surfaceRaised, in: RoundedRectangle(cornerRadius: 11))

        VStack(alignment: .leading, spacing: 5) {
          HStack(spacing: 8) {
            Text("Steam for Windows")
              .font(.system(size: 15, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            StatusPill(
              model.steamIsRunning
                ? "RUNNING"
                : (model.steamStatus?.installed == true ? "READY" : "NOT INSTALLED"),
              tone: model.steamStatus?.installed == true ? .success : .neutral
            )
          }
          Text(
            model.steamStatus?.installed == true
              ? "\(model.steamGames.count) installed games"
              : "Complete initial setup from Home"
          )
          .font(.system(size: 11.5))
          .foregroundStyle(DarwinPalette.textSecondary)
          if model.steamStatus?.installed == true {
            Text(
              model.steamUiRestartRequired
                ? "Web UI · restart required for current compatibility policy"
                : "Web UI · software rendering"
            )
            .font(.system(size: 10.5, weight: .medium))
            .foregroundStyle(
              model.steamUiRestartRequired ? DarwinPalette.warning : DarwinPalette.textTertiary
            )
          }
          if let path = model.steamStatus?.steamPath {
            Text(path)
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(DarwinPalette.textTertiary)
              .textSelection(.enabled)
              .lineLimit(2)
          }
        }
        Spacer(minLength: 12)

        if model.steamStatus?.installed == true {
          HStack(spacing: 8) {
            if model.steamIsRunning {
              Button {
                model.restartSteamUI()
              } label: {
                Text("Restart UI")
              }
              .buttonStyle(SecondaryActionButtonStyle())
            } else {
              Button {
                model.openSteam()
              } label: {
                Text("Open")
              }
              .buttonStyle(SecondaryActionButtonStyle())
            }

            Button(role: .destructive) {
              Task { await model.resetSteam() }
            } label: {
              Text("Reset Prefix")
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(model.steamIsRunning)
          }
        }
      }
    }
  }

  private var graphicsSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading(
        "Graphics", subtitle: "Default translation path; game profiles may override it")
      SurfaceCard {
        VStack(alignment: .leading, spacing: 17) {
          HStack {
            VStack(alignment: .leading, spacing: 3) {
              Text("Default backend")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
              Text("Automatic prefers DXMT for supported D3D10/11 games.")
                .font(.system(size: 10.5))
                .foregroundStyle(DarwinPalette.textTertiary)
            }
            Spacer()
            Picker("Default backend", selection: $graphicsBackend) {
              ForEach(GraphicsBackendPreference.allCases) { backend in
                Text(backend.displayName).tag(backend)
              }
            }
            .labelsHidden()
            .frame(width: 180)
          }

          Divider().overlay(DarwinPalette.border)

          HStack(alignment: .top, spacing: 13) {
            Image(systemName: "rectangle.3.group")
              .font(.system(size: 18, weight: .medium))
              .foregroundStyle(DarwinPalette.info)
              .frame(width: 42, height: 42)
              .background(DarwinPalette.surfaceRaised, in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 5) {
              HStack(spacing: 8) {
                Text("DXMT")
                  .font(.system(size: 14, weight: .semibold))
                  .foregroundStyle(DarwinPalette.textPrimary)
                StatusPill(
                  model.dxmtStatus?.installed == true ? "INSTALLED" : "OPTIONAL",
                  tone: model.dxmtStatus?.installed == true ? .success : .neutral
                )
              }
              Text(dxmtDescription)
                .font(.system(size: 11))
                .foregroundStyle(DarwinPalette.textSecondary)
                .textSelection(.enabled)
            }
            Spacer()
            if model.dxmtStatus?.installed == true {
              Button(role: .destructive) {
                Task { await model.removeDxmt() }
              } label: {
                Text("Remove")
              }
              .buttonStyle(SecondaryActionButtonStyle())
            }
          }

          Picker("Package mode", selection: $dxmtMode) {
            ForEach(DxmtMode.allCases) { mode in
              Text(mode.displayName).tag(mode)
            }
          }
          .pickerStyle(.segmented)

          Button {
            chooseDxmtPackage()
          } label: {
            Label(
              model.dxmtStatus?.installed == true ? "Replace DXMT Package" : "Install DXMT Package",
              systemImage: "square.and.arrow.down"
            )
          }
          .buttonStyle(SecondaryActionButtonStyle())
        }
      }
    }
  }

  private var advancedSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeading(
        "Advanced", subtitle: "Override Wine discovery only when you need a custom build")
      SurfaceCard {
        VStack(alignment: .leading, spacing: 9) {
          Text("Wine executable override")
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(DarwinPalette.textPrimary)
          HStack(spacing: 8) {
            TextField("Automatic", text: $winePath)
              .textFieldStyle(.plain)
              .font(.system(size: 11.5, design: .monospaced))
              .padding(.horizontal, 11)
              .frame(height: 38)
              .background(DarwinPalette.console, in: RoundedRectangle(cornerRadius: 9))
              .overlay {
                RoundedRectangle(cornerRadius: 9).stroke(DarwinPalette.border, lineWidth: 1)
              }
            Button {
              chooseWine()
            } label: {
              Label("Browse", systemImage: "folder")
            }
            .buttonStyle(SecondaryActionButtonStyle())
          }
          Text("Leave empty to use managed Wine or automatic discovery.")
            .font(.system(size: 10.5))
            .foregroundStyle(DarwinPalette.textTertiary)
        }
      }
    }
  }

  private var footer: some View {
    HStack(spacing: 8) {
      Spacer()
      Button("Cancel") {
        dismiss()
      }
      .buttonStyle(SecondaryActionButtonStyle())
      Button {
        Task {
          await model.saveSettings(
            AppSettings(winePath: winePath, graphicsBackend: graphicsBackend)
          )
        }
      } label: {
        Text("Save")
      }
      .buttonStyle(PrimaryActionButtonStyle())
    }
    .padding(18)
  }

  private var dxmtDescription: String {
    guard let status = model.dxmtStatus, status.installed else {
      return "Direct3D 10/11 → Metal. WineD3D stays available without it."
    }
    let mode = status.mode?.displayName ?? "Unknown mode"
    let source = status.sourceName ?? status.root ?? "Managed component"
    return "\(mode) · \(source)"
  }

  private func chooseWine() {
    let panel = NSOpenPanel()
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true
    panel.message = "Select the Wine executable"
    if panel.runModal() == .OK, let url = panel.url {
      winePath = url.path
    }
  }

  private func chooseDxmtPackage() {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true
    panel.message = "Select the extracted DXMT package containing x86_64-unix and x86_64-windows"
    if panel.runModal() == .OK, let url = panel.url {
      Task { await model.installDxmt(source: url, mode: dxmtMode) }
    }
  }
}
