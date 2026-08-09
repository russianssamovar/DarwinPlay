import Foundation
import SwiftUI

enum DarwinPalette {
  static let background = Color(red: 0.051, green: 0.075, blue: 0.090)
  static let backgroundElevated = Color(red: 0.071, green: 0.106, blue: 0.125)
  static let surface = Color(red: 0.094, green: 0.137, blue: 0.157)
  static let surfaceRaised = Color(red: 0.125, green: 0.176, blue: 0.200)
  static let surfaceHover = Color(red: 0.145, green: 0.196, blue: 0.220)
  static let border = Color(red: 0.176, green: 0.231, blue: 0.251)
  static let borderStrong = Color(red: 0.240, green: 0.302, blue: 0.325)
  static let accent = Color(red: 0.561, green: 0.729, blue: 0.388)
  static let accentSoft = Color(red: 0.675, green: 0.792, blue: 0.514)
  static let accentMuted = Color(red: 0.392, green: 0.514, blue: 0.286)
  static let success = Color(red: 0.431, green: 0.682, blue: 0.478)
  static let warning = Color(red: 0.839, green: 0.659, blue: 0.361)
  static let danger = Color(red: 0.824, green: 0.431, blue: 0.431)
  static let info = Color(red: 0.435, green: 0.639, blue: 0.741)
  static let textPrimary = Color(red: 0.949, green: 0.965, blue: 0.953)
  static let textSecondary = Color(red: 0.659, green: 0.706, blue: 0.718)
  static let textTertiary = Color(red: 0.435, green: 0.490, blue: 0.510)
  static let console = Color(red: 0.031, green: 0.047, blue: 0.055)
}

struct LauncherBackground: View {
  var body: some View {
    LinearGradient(
      colors: [DarwinPalette.backgroundElevated.opacity(0.72), DarwinPalette.background],
      startPoint: .top,
      endPoint: .bottom
    )
    .overlay(alignment: .topTrailing) {
      LinearGradient(
        colors: [DarwinPalette.info.opacity(0.045), .clear],
        startPoint: .topTrailing,
        endPoint: .bottomLeading
      )
      .frame(width: 620, height: 420)
    }
    .ignoresSafeArea()
  }
}

struct DarwinMark: View {
  var size: CGFloat = 38

  var body: some View {
    Canvas { context, canvasSize in
      let scale = min(canvasSize.width, canvasSize.height) / 64
      context.scaleBy(x: scale, y: scale)

      var backLayer = Path()
      backLayer.addEllipse(in: CGRect(x: 9, y: 20, width: 35, height: 28))
      context.fill(backLayer, with: .color(DarwinPalette.accentMuted))

      var middleLayer = Path()
      middleLayer.addEllipse(in: CGRect(x: 15, y: 15, width: 34, height: 31))
      context.fill(middleLayer, with: .color(DarwinPalette.accent))

      var head = Path()
      head.move(to: CGPoint(x: 24, y: 45))
      head.addCurve(
        to: CGPoint(x: 43, y: 18),
        control1: CGPoint(x: 19, y: 31),
        control2: CGPoint(x: 27, y: 18)
      )
      head.addCurve(
        to: CGPoint(x: 50, y: 35),
        control1: CGPoint(x: 52, y: 19),
        control2: CGPoint(x: 55, y: 28)
      )
      head.addCurve(
        to: CGPoint(x: 24, y: 45),
        control1: CGPoint(x: 43, y: 44),
        control2: CGPoint(x: 32, y: 48)
      )
      context.fill(head, with: .color(DarwinPalette.textPrimary))

      var beak = Path()
      beak.move(to: CGPoint(x: 47, y: 26))
      beak.addLine(to: CGPoint(x: 61, y: 31))
      beak.addLine(to: CGPoint(x: 48, y: 35))
      beak.closeSubpath()
      context.fill(beak, with: .color(DarwinPalette.accentSoft))

      let eye = Path(ellipseIn: CGRect(x: 42, y: 24, width: 4.5, height: 4.5))
      context.fill(eye, with: .color(DarwinPalette.background))

      var layerCut = Path()
      layerCut.move(to: CGPoint(x: 13, y: 45))
      layerCut.addCurve(
        to: CGPoint(x: 32, y: 50),
        control1: CGPoint(x: 17, y: 38),
        control2: CGPoint(x: 25, y: 43)
      )
      context.stroke(
        layerCut,
        with: .color(DarwinPalette.background.opacity(0.7)),
        style: StrokeStyle(lineWidth: 2.5, lineCap: .round)
      )
    }
    .frame(width: size, height: size)
    .accessibilityHidden(true)
  }
}

struct SurfaceCard<Content: View>: View {
  let padding: CGFloat
  let content: Content

  init(padding: CGFloat = 18, @ViewBuilder content: () -> Content) {
    self.padding = padding
    self.content = content()
  }

  var body: some View {
    content
      .padding(padding)
      .background(DarwinPalette.surface.opacity(0.78), in: RoundedRectangle(cornerRadius: 18))
      .overlay {
        RoundedRectangle(cornerRadius: 18)
          .stroke(DarwinPalette.border.opacity(0.72), lineWidth: 1)
      }
  }
}

struct SectionHeading: View {
  let title: String
  let subtitle: String?

  init(_ title: String, subtitle: String? = nil) {
    self.title = title
    self.subtitle = subtitle
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(title)
        .font(.system(size: 20, weight: .semibold))
        .foregroundStyle(DarwinPalette.textPrimary)
      if let subtitle {
        Text(subtitle)
          .font(.system(size: 12.5))
          .foregroundStyle(DarwinPalette.textSecondary)
      }
    }
  }
}

struct StatusPill: View {
  enum Tone {
    case neutral
    case success
    case warning
    case danger
    case accent
    case info
  }

  let text: String
  let systemImage: String?
  let tone: Tone

  init(_ text: String, systemImage: String? = nil, tone: Tone = .neutral) {
    self.text = text
    self.systemImage = systemImage
    self.tone = tone
  }

  var body: some View {
    HStack(spacing: 6) {
      if let systemImage {
        Image(systemName: systemImage)
          .font(.system(size: 9, weight: .bold))
      }
      Text(text)
        .font(.system(size: 10.5, weight: .semibold))
        .lineLimit(1)
    }
    .foregroundStyle(foreground)
    .padding(.horizontal, 9)
    .padding(.vertical, 5)
    .background(foreground.opacity(0.10), in: Capsule())
  }

  private var foreground: Color {
    switch tone {
    case .neutral: DarwinPalette.textSecondary
    case .success: DarwinPalette.success
    case .warning: DarwinPalette.warning
    case .danger: DarwinPalette.danger
    case .accent: DarwinPalette.accentSoft
    case .info: DarwinPalette.info
    }
  }
}

struct PrimaryActionButtonStyle: ButtonStyle {
  @Environment(\.isEnabled) private var isEnabled

  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(.system(size: 13, weight: .semibold))
      .foregroundStyle(DarwinPalette.background)
      .padding(.horizontal, 16)
      .frame(minHeight: 38)
      .background(
        configuration.isPressed ? DarwinPalette.accentMuted : DarwinPalette.accent,
        in: RoundedRectangle(cornerRadius: 10)
      )
      .scaleEffect(configuration.isPressed ? 0.985 : 1)
      .opacity(isEnabled ? 1 : 0.42)
  }
}

struct SecondaryActionButtonStyle: ButtonStyle {
  @Environment(\.isEnabled) private var isEnabled

  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(.system(size: 13, weight: .semibold))
      .foregroundStyle(DarwinPalette.textPrimary)
      .padding(.horizontal, 14)
      .frame(minHeight: 38)
      .background(
        configuration.isPressed ? DarwinPalette.surfaceHover : DarwinPalette.surface,
        in: RoundedRectangle(cornerRadius: 10)
      )
      .overlay {
        RoundedRectangle(cornerRadius: 10)
          .stroke(DarwinPalette.border, lineWidth: 1)
      }
      .opacity(isEnabled ? 1 : 0.42)
  }
}

struct ActionButtonStyle: ButtonStyle {
  @Environment(\.isEnabled) private var isEnabled
  let emphasized: Bool

  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(.system(size: 13, weight: .semibold))
      .foregroundStyle(emphasized ? DarwinPalette.background : DarwinPalette.textPrimary)
      .padding(.horizontal, 14)
      .frame(minHeight: 38)
      .background(
        emphasized
          ? (configuration.isPressed ? DarwinPalette.accentMuted : DarwinPalette.accent)
          : (configuration.isPressed ? DarwinPalette.surfaceHover : DarwinPalette.surface),
        in: RoundedRectangle(cornerRadius: 10)
      )
      .overlay {
        if !emphasized {
          RoundedRectangle(cornerRadius: 10)
            .stroke(DarwinPalette.border, lineWidth: 1)
        }
      }
      .opacity(isEnabled ? 1 : 0.42)
  }
}

struct IconActionButtonStyle: ButtonStyle {
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .foregroundStyle(DarwinPalette.textSecondary)
      .frame(width: 36, height: 36)
      .background(
        configuration.isPressed ? DarwinPalette.surfaceHover : Color.clear,
        in: Circle()
      )
  }
}

struct LauncherPageHeader: View {
  let title: String
  let subtitle: String?
  let actions: AnyView?

  init(title: String, subtitle: String? = nil, actions: AnyView? = nil) {
    self.title = title
    self.subtitle = subtitle
    self.actions = actions
  }

  var body: some View {
    HStack(alignment: .bottom, spacing: 20) {
      VStack(alignment: .leading, spacing: 5) {
        Text(title)
          .font(.system(size: 30, weight: .semibold))
          .foregroundStyle(DarwinPalette.textPrimary)
        if let subtitle {
          Text(subtitle)
            .font(.system(size: 13))
            .foregroundStyle(DarwinPalette.textSecondary)
        }
      }
      Spacer(minLength: 24)
      actions
    }
  }
}

struct RuntimeStateBadge: View {
  let name: String
  let value: String
  let ready: Bool

  var body: some View {
    HStack(spacing: 8) {
      Circle()
        .fill(ready ? DarwinPalette.accent : DarwinPalette.warning)
        .frame(width: 6, height: 6)
      Text(name)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(DarwinPalette.textSecondary)
      Text(value)
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(DarwinPalette.textPrimary)
        .lineLimit(1)
    }
  }
}

struct RuntimeConsoleView: View {
  let entries: [ConsoleEntry]
  let isRunning: Bool

  var body: some View {
    VStack(spacing: 0) {
      HStack(spacing: 8) {
        Circle()
          .fill(isRunning ? DarwinPalette.accent : DarwinPalette.textTertiary)
          .frame(width: 6, height: 6)
        Text(isRunning ? "LIVE" : "IDLE")
          .font(.system(size: 9, weight: .bold, design: .monospaced))
          .tracking(1.2)
          .foregroundStyle(isRunning ? DarwinPalette.accentSoft : DarwinPalette.textTertiary)
        Spacer()
      }
      .padding(.horizontal, 14)
      .frame(height: 36)

      Rectangle().fill(DarwinPalette.border.opacity(0.55)).frame(height: 1)

      ScrollViewReader { proxy in
        ScrollView {
          LazyVStack(alignment: .leading, spacing: 6) {
            if entries.isEmpty {
              Text("waiting for runtime output")
                .foregroundStyle(DarwinPalette.textTertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
              ForEach(entries) { entry in
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                  Text(entry.timestamp.formatted(date: .omitted, time: .standard))
                    .foregroundStyle(DarwinPalette.textTertiary)
                  Text(entry.component.rawValue.lowercased())
                    .foregroundStyle(DarwinPalette.info.opacity(0.9))
                    .frame(width: 62, alignment: .leading)
                  Text(entry.message)
                    .foregroundStyle(consoleColor(entry.level))
                    .textSelection(.enabled)
                }
                .id(entry.id)
              }
            }
          }
          .font(.system(size: 11.5, design: .monospaced))
          .padding(14)
        }
        .onChange(of: entries.count) { _, _ in
          if let last = entries.last {
            proxy.scrollTo(last.id, anchor: .bottom)
          }
        }
      }
    }
    .background(DarwinPalette.console, in: RoundedRectangle(cornerRadius: 14))
    .overlay {
      RoundedRectangle(cornerRadius: 14)
        .stroke(DarwinPalette.border.opacity(0.7), lineWidth: 1)
    }
  }

  private func consoleColor(_ level: ConsoleLevel) -> Color {
    switch level {
    case .info: DarwinPalette.textSecondary
    case .success: DarwinPalette.accentSoft
    case .warning: DarwinPalette.warning
    case .error: DarwinPalette.danger
    }
  }
}

struct OperationProgressView: View {
  let state: OperationProgressState

  var body: some View {
    VStack(alignment: .leading, spacing: 7) {
      HStack(spacing: 8) {
        Text(state.phase.uppercased())
          .font(.system(size: 9, weight: .bold))
          .tracking(1.1)
          .foregroundStyle(DarwinPalette.accentSoft)
        Spacer()
        if let overall = state.overallProgress {
          percentageText(overall)
        } else if let progress = state.progress {
          percentageText(progress)
        }
      }

      if let overall = state.overallProgress {
        ProgressView(value: overall.clamped(to: 0...1))
          .progressViewStyle(.linear)
          .tint(DarwinPalette.accent)
      } else if let progress = state.progress {
        ProgressView(value: progress.clamped(to: 0...1))
          .progressViewStyle(.linear)
          .tint(DarwinPalette.accent)
      } else {
        ProgressView()
          .progressViewStyle(.linear)
          .tint(DarwinPalette.accent)
      }

      if let progress = state.progress, state.overallProgress != nil,
        state.currentBytes != nil
      {
        HStack(spacing: 8) {
          Text("CURRENT DOWNLOAD")
            .font(.system(size: 8.5, weight: .bold))
            .tracking(0.9)
            .foregroundStyle(DarwinPalette.textTertiary)
          Spacer()
          percentageText(progress, size: 9.5)
        }
        ProgressView(value: progress.clamped(to: 0...1))
          .progressViewStyle(.linear)
          .tint(DarwinPalette.accentSoft)
      }

      HStack(alignment: .firstTextBaseline, spacing: 8) {
        Text(state.message)
          .font(.system(size: 10.5))
          .foregroundStyle(DarwinPalette.textSecondary)
          .lineLimit(2)
        Spacer(minLength: 8)
        if let bytesText {
          Text(bytesText)
            .font(.system(size: 9.5, design: .monospaced))
            .foregroundStyle(DarwinPalette.textTertiary)
        }
      }
    }
    .padding(10)
    .background(DarwinPalette.surface.opacity(0.72), in: RoundedRectangle(cornerRadius: 10))
  }

  private func percentageText(_ value: Double, size: CGFloat = 10.5) -> some View {
    Text("\(Int((value.clamped(to: 0...1) * 100).rounded()))%")
      .font(.system(size: size, weight: .semibold, design: .monospaced))
      .foregroundStyle(DarwinPalette.textSecondary)
  }

  private var bytesText: String? {
    guard let current = state.currentBytes else { return nil }
    let formatter = ByteCountFormatter()
    formatter.countStyle = .file
    formatter.allowedUnits = [.useMB, .useGB]
    formatter.includesUnit = true
    formatter.isAdaptive = true
    let currentText = formatter.string(fromByteCount: Int64(current))
    guard let total = state.totalBytes, total > 0 else { return currentText }
    return "\(currentText) / \(formatter.string(fromByteCount: Int64(total)))"
  }
}

extension Comparable {
  fileprivate func clamped(to limits: ClosedRange<Self>) -> Self {
    min(max(self, limits.lowerBound), limits.upperBound)
  }
}

struct ArtworkPlaceholder: View {
  let seed: UInt64
  let symbol: String

  var body: some View {
    ZStack {
      LinearGradient(
        colors: [seedColor.opacity(0.65), DarwinPalette.surface],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
      )
      Image(systemName: symbol)
        .font(.system(size: 42, weight: .medium))
        .foregroundStyle(DarwinPalette.textPrimary.opacity(0.78))
    }
  }

  private var seedColor: Color {
    let hue = 0.48 + Double(seed % 120) / 1800.0
    return Color(hue: hue, saturation: 0.28, brightness: 0.58)
  }
}
