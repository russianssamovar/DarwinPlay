import AppKit
import DarwinPlayCore
import SwiftUI

struct SettingsView: View {
  @Environment(\.dismiss) private var dismiss
  @Bindable var model: AppModel
  @State private var graphicsBackend: GraphicsBackendPreference
  @State private var steamUiBackend: GraphicsBackendPreference
  @State private var dxmtMode: DxmtMode = .builtin

  init(model: AppModel) {
    self.model = model
    _graphicsBackend = State(initialValue: model.settings.graphicsBackend)
    _steamUiBackend = State(initialValue: model.settings.steamUiBackend)
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
        Text("DarwinWine, Steam, and graphics")
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
      SectionHeading("Runtime", subtitle: "DarwinPlay uses DarwinWine exclusively")
      VStack(spacing: 10) {
        darwinWineCard
        steamRuntimeCard
      }
    }
  }

  private var darwinWineCard: some View {
    let status = model.runtimeStatus
    let ready = status?.ready == true
    let installed = status?.installed == true

    return SurfaceCard {
      HStack(alignment: .top, spacing: 14) {
        DarwinMark(size: 42)
        VStack(alignment: .leading, spacing: 5) {
          HStack(spacing: 8) {
            Text("DarwinWine")
              .font(.system(size: 15, weight: .semibold))
              .foregroundStyle(DarwinPalette.textPrimary)
            StatusPill(
              ready ? "READY" : (installed ? "NEEDS REINSTALL" : "NOT INSTALLED"),
              tone: ready ? .success : .warning
            )
          }

          Text(status?.darwinWineVersion ?? "DarwinWine cx26.3-dp5 or newer")
            .font(.system(size: 11.5))
            .foregroundStyle(DarwinPalette.textSecondary)

          if let wineVersion = status?.wineVersion {
            Text(
              "Wine \(wineVersion) · \(status?.architecture ?? "unknown") · \(status?.channel ?? "unknown")"
            )
            .font(.system(size: 10.5, weight: .medium))
            .foregroundStyle(DarwinPalette.textTertiary)
          }

          if ready {
            HStack(spacing: 10) {
              Label(
                status?.steamValidated == true ? "Steam verified" : "Steam validation pending",
                systemImage: status?.steamValidated == true ? "checkmark.seal.fill" : "clock"
              )
              Label(
                status?.steamLoginValidated == true ? "Login verified" : "Login validation pending",
                systemImage: status?.steamLoginValidated == true ? "checkmark.seal.fill" : "clock"
              )
            }
            .font(.system(size: 10.5, weight: .medium))
            .foregroundStyle(DarwinPalette.textTertiary)
          }

          if let path = status?.winePath {
            Text(path)
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(DarwinPalette.textTertiary)
              .textSelection(.enabled)
              .lineLimit(2)
          }

          if let error = status?.probeError, !ready {
            Text(error)
              .font(.system(size: 10, design: .monospaced))
              .foregroundStyle(DarwinPalette.warning)
              .textSelection(.enabled)
              .lineLimit(4)
          }

          if let progress = model.darwinWineProgress {
            OperationProgressView(state: progress)
              .padding(.top, 5)
          }
        }

        Spacer(minLength: 12)

        HStack(spacing: 8) {
          if ready {
            darwinWineInstallButton(installed: installed)
              .buttonStyle(SecondaryActionButtonStyle())
          } else {
            darwinWineInstallButton(installed: installed)
              .buttonStyle(PrimaryActionButtonStyle())
          }

          if installed {
            Button(role: .destructive) {
              Task { await model.removeDarwinWine() }
            } label: {
              Text("Remove")
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(
              model.isManagingDarwinWine || model.steamIsRunning || !model.runningGameIDs.isEmpty)
          }
        }
      }
    }
  }

  private func darwinWineInstallButton(installed: Bool) -> some View {
    Button {
      Task { await model.installDarwinWine() }
    } label: {
      Text(
        model.isManagingDarwinWine
          ? "Installing…" : (installed ? "Install Update…" : "Install Runtime…"))
    }
    .disabled(
      model.isManagingDarwinWine || model.steamIsRunning || !model.runningGameIDs.isEmpty
    )
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
              model.steamStatus?.prefixRuntimeCompatible == false
                ? "RUNTIME RESET REQUIRED"
                : (model.steamIsRunning
                  ? "RUNNING"
                  : (model.steamStatus?.installed == true ? "READY" : "NOT INSTALLED")),
              tone:
                model.steamStatus?.prefixRuntimeCompatible == false
                ? .warning : (model.steamStatus?.installed == true ? .success : .neutral)
            )
          }
          Text(
            model.steamStatus?.installed == true
              ? "\(model.steamGames.count) installed games"
              : "Complete initial setup from Home"
          )
          .font(.system(size: 11.5))
          .foregroundStyle(DarwinPalette.textSecondary)
          if model.steamStatus?.installed == true,
            model.steamStatus?.prefixRuntimeCompatible == false
          {
            Text(
              "Prefix runtime · \(model.steamStatus?.prefixRuntimeVersion ?? "unknown") · DarwinWine \(model.runtimeStatus?.wineVersion ?? "unknown"). Reset the Steam prefix after a runtime-incompatible update."
            )
            .font(.system(size: 10.5))
            .foregroundStyle(DarwinPalette.warning)
            .fixedSize(horizontal: false, vertical: true)
          }
          if model.steamStatus?.installed == true {
            Text(
              model.steamUiRestartRequired
                ? "UI renderer · restart required"
                : "UI renderer · \(steamUiBackendDescription)"
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

          if let progress = model.steamInstallProgress {
            OperationProgressView(state: progress)
              .padding(.top, 5)
          }
        }
        Spacer(minLength: 12)

        if model.steamStatus?.installed == true {
          HStack(spacing: 8) {
            if model.steamIsRunning {
              Button {
                Task { await model.stopSteam() }
              } label: {
                Text("Stop")
              }
              .buttonStyle(SecondaryActionButtonStyle())

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
              .disabled(model.steamStatus?.prefixRuntimeCompatible == false)
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
              Text("Default game backend")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
              Text("Automatic prefers DXMT for supported D3D10/11 games.")
                .font(.system(size: 10.5))
                .foregroundStyle(DarwinPalette.textTertiary)
            }
            Spacer()
            Picker("Default game backend", selection: $graphicsBackend) {
              ForEach(GraphicsBackendPreference.allCases) { backend in
                Text(backend.displayName).tag(backend)
              }
            }
            .labelsHidden()
            .frame(width: 180)
          }

          HStack {
            VStack(alignment: .leading, spacing: 3) {
              Text("Steam UI backend")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(DarwinPalette.textPrimary)
              Text("Automatic uses DXMT when installed and WineD3D otherwise.")
                .font(.system(size: 10.5))
                .foregroundStyle(DarwinPalette.textTertiary)
            }
            Spacer()
            Picker("Steam UI backend", selection: $steamUiBackend) {
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
              .disabled(model.isManagingDxmt || model.steamIsRunning)
            }
          }

          HStack(spacing: 8) {
            Button {
              Task {
                if model.dxmtStatus?.installed == true {
                  await model.updateDxmt()
                } else {
                  await model.installLatestDxmt()
                }
              }
            } label: {
              Label(
                model.isManagingDxmt
                  ? "Working…"
                  : (model.dxmtStatus?.installed == true ? "Update DXMT" : "Install Latest DXMT"),
                systemImage: "arrow.down.circle"
              )
            }
            .buttonStyle(PrimaryActionButtonStyle())
            .disabled(model.isManagingDxmt || model.steamIsRunning)

            Menu {
              Picker("Package mode", selection: $dxmtMode) {
                ForEach(DxmtMode.allCases) { mode in
                  Text(mode.displayName).tag(mode)
                }
              }
              Button("Install from Folder…") {
                chooseDxmtPackage()
              }
            } label: {
              Label("Manual", systemImage: "folder")
            }
            .buttonStyle(SecondaryActionButtonStyle())
            .disabled(model.isManagingDxmt || model.steamIsRunning)
          }

          if model.steamIsRunning {
            Text("Stop Steam before installing, updating, or removing DXMT.")
              .font(.system(size: 10.5))
              .foregroundStyle(DarwinPalette.warning)
          }
        }
      }
    }
  }

  private var footer: some View {
    HStack(spacing: 8) {
      Spacer()
      Button("Cancel") { dismiss() }
        .buttonStyle(SecondaryActionButtonStyle())
      Button {
        Task {
          await model.saveSettings(
            AppSettings(
              graphicsBackend: graphicsBackend,
              steamUiBackend: steamUiBackend
            )
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
    let version = status.version ?? "custom"
    let source =
      status.managed ? "Managed from official release" : (status.sourceName ?? "Manual package")
    return "\(version) · \(mode) · \(source)"
  }

  private var steamUiBackendDescription: String {
    let backend = model.steamStatus?.uiBackend ?? model.effectiveSteamUiBackend
    if backend == .dxmt, let version = model.steamStatus?.uiDxmtVersion ?? model.dxmtStatus?.version
    {
      if model.steamStatus?.running == true, model.steamStatus?.uiBackendVerified == false {
        return "DXMT \(version) requested · not confirmed"
      }
      return "DXMT \(version) → Metal"
    }
    return backend.displayName
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
