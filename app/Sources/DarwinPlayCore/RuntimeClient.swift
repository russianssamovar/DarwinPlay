import Foundation

public enum RuntimeClientError: Error, LocalizedError, Sendable {
  case runtimeNotFound
  case commandFailed(Int32, String)
  case invalidOutput(String)

  public var errorDescription: String? {
    switch self {
    case .runtimeNotFound:
      "darwinplay-runtime was not found next to the app executable or in DARWINPLAY_RUNTIME"
    case .commandFailed(let code, let message):
      message.isEmpty ? "Runtime command failed with exit code \(code)" : message
    case .invalidOutput(let output):
      "Runtime returned invalid output: \(output)"
    }
  }
}

public struct RuntimeClient: Sendable {
  public let executableURL: URL

  public init(executableURL: URL? = nil) throws {
    if let executableURL {
      self.executableURL = executableURL
      return
    }

    if let path = ProcessInfo.processInfo.environment["DARWINPLAY_RUNTIME"],
      FileManager.default.isExecutableFile(atPath: path)
    {
      self.executableURL = URL(fileURLWithPath: path)
      return
    }

    if let appExecutable = Bundle.main.executableURL {
      let sibling = appExecutable.deletingLastPathComponent().appendingPathComponent(
        "darwinplay-runtime")
      if FileManager.default.isExecutableFile(atPath: sibling.path) {
        self.executableURL = sibling
        return
      }
    }

    throw RuntimeClientError.runtimeNotFound
  }

  public func wineStatus(winePath: String?) async throws -> WineStatus {
    try await runJSON(arguments: globalArguments(winePath) + ["wine", "status", "--json"])
  }

  public func installWine() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["wine", "install", "--json"])
  }

  public func reinstallWine() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["wine", "reinstall", "--json"])
  }

  public func removeWine() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["wine", "remove", "--json"])
  }

  public func doctor(winePath: String?) async throws -> DoctorReport {
    try await runJSON(arguments: globalArguments(winePath) + ["doctor", "--json"])
  }

  public func inspect(executable: URL) async throws -> PEReport {
    try await runJSON(arguments: ["inspect", executable.path, "--json"])
  }

  public func dxmtStatus() async throws -> DxmtStatus {
    try await runJSON(arguments: ["graphics", "dxmt", "status", "--json"])
  }

  public func installDxmt(source: URL, mode: DxmtMode) async throws -> DxmtStatus {
    try await runJSON(arguments: [
      "graphics", "dxmt", "install",
      "--source", source.path,
      "--mode", mode.rawValue,
      "--json",
    ])
  }

  public func removeDxmt() async throws {
    _ = try await run(arguments: ["graphics", "dxmt", "remove"])
  }

  public func steamStatus(winePath: String? = nil) async throws -> SteamStatus {
    try await runJSON(arguments: globalArguments(winePath) + ["steam", "status", "--json"])
  }

  public func installSteam(winePath: String?, installer: URL? = nil) async throws -> SteamStatus {
    var arguments = globalArguments(winePath) + ["steam", "install", "--json"]
    if let installer {
      arguments += ["--installer", installer.path]
    }
    return try await runJSON(arguments: arguments)
  }

  public func steamGames() async throws -> SteamLibrary {
    try await runJSON(arguments: ["steam", "games", "--json"])
  }

  public func steamProfile(
    appID: UInt32,
    fallbackBackend: GraphicsBackendPreference
  ) async throws -> SteamCompatibilityProfile {
    try await runJSON(arguments: [
      "steam", "profile", "show",
      "--app-id", String(appID),
      "--fallback-backend", fallbackBackend.rawValue,
      "--json",
    ])
  }

  public func saveSteamProfile(
    appID: UInt32,
    backend: SteamBackendOverride,
    executable: String?,
    launchArguments: [String],
    fallbackBackend: GraphicsBackendPreference
  ) async throws -> SteamCompatibilityProfile {
    var arguments = [
      "steam", "profile", "set",
      "--app-id", String(appID),
      "--backend", backend.rawValue,
      "--fallback-backend", fallbackBackend.rawValue,
      "--json",
    ]
    if let executable {
      arguments += ["--executable", executable]
    }
    for argument in launchArguments {
      arguments += ["--launch-argument", argument]
    }
    return try await runJSON(arguments: arguments)
  }

  public func resetSteamProfile(
    appID: UInt32,
    fallbackBackend: GraphicsBackendPreference
  ) async throws -> SteamCompatibilityProfile {
    try await runJSON(arguments: [
      "steam", "profile", "reset",
      "--app-id", String(appID),
      "--fallback-backend", fallbackBackend.rawValue,
      "--json",
    ])
  }

  public func startSteam(
    winePath: String?,
    backend: GraphicsBackendPreference = .wined3d
  ) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: globalArguments(winePath) + [
        "steam", "start",
        "--backend", backend.rawValue,
        "--json",
      ])
  }

  public func restartSteam(
    winePath: String?,
    backend: GraphicsBackendPreference = .wined3d
  ) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: globalArguments(winePath) + [
        "steam", "restart",
        "--backend", backend.rawValue,
        "--json",
      ])
  }

  public func launchSteamGame(
    appID: UInt32,
    winePath: String?,
    backend: GraphicsBackendPreference
  ) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: globalArguments(winePath) + [
        "steam", "run",
        "--app-id", String(appID),
        "--backend", backend.rawValue,
        "--json",
      ])
  }

  public func stopSteam(winePath: String?) async throws {
    _ = try await run(arguments: globalArguments(winePath) + ["steam", "stop"])
  }

  public func resetSteam(winePath: String?) async throws {
    _ = try await run(arguments: globalArguments(winePath) + ["steam", "reset"])
  }

  public func resetPrefix(gameID: UUID, winePath: String?) async throws {
    _ = try await run(
      arguments: globalArguments(winePath) + ["prefix", "reset", "--game-id", gameID.uuidString])
  }

  public func stop(gameID: UUID, winePath: String?) async throws {
    _ = try await run(
      arguments: globalArguments(winePath) + ["stop", "--game-id", gameID.uuidString])
  }

  public func launch(
    game: GameRecord,
    winePath: String?,
    backend: GraphicsBackendPreference
  ) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: globalArguments(winePath) + [
        "launch",
        "--game-id", game.id.uuidString,
        "--executable", game.executablePath,
        "--backend", backend.rawValue,
        "--json",
      ])
  }

  private func stream(arguments: [String]) -> AsyncThrowingStream<RuntimeEvent, Error> {
    let executable = executableURL
    return AsyncThrowingStream { continuation in
      let task = Task.detached(priority: .userInitiated) {
        let process = Process()
        let stdout = Pipe()
        let stderr = Pipe()
        process.executableURL = executable
        process.arguments = arguments
        process.standardOutput = stdout
        process.standardError = stderr

        do {
          try process.run()
          let stderrTask = Task.detached {
            stderr.fileHandleForReading.readDataToEndOfFile()
          }
          var framer = LineFramer()
          while let chunk = try stdout.fileHandleForReading.read(upToCount: 4096), !chunk.isEmpty {
            for line in framer.append(chunk) {
              do {
                continuation.yield(
                  try JSONDecoder().decode(RuntimeEvent.self, from: Data(line.utf8)))
              } catch {
                continuation.finish(throwing: RuntimeClientError.invalidOutput(line))
                if process.isRunning {
                  process.terminate()
                }
                return
              }
            }
          }
          if let line = framer.finish() {
            do {
              continuation.yield(try JSONDecoder().decode(RuntimeEvent.self, from: Data(line.utf8)))
            } catch {
              continuation.finish(throwing: RuntimeClientError.invalidOutput(line))
              if process.isRunning {
                process.terminate()
              }
              return
            }
          }
          process.waitUntilExit()
          let errorData = await stderrTask.value
          let errorText = String(decoding: errorData, as: UTF8.self).trimmingCharacters(
            in: .whitespacesAndNewlines)
          if process.terminationStatus == 0 {
            continuation.finish()
          } else {
            continuation.finish(
              throwing: RuntimeClientError.commandFailed(process.terminationStatus, errorText))
          }
        } catch {
          continuation.finish(throwing: error)
        }
      }

      continuation.onTermination = { _ in
        task.cancel()
      }
    }
  }

  private func runJSON<T: Decodable & Sendable>(arguments: [String]) async throws -> T {
    let data = try await run(arguments: arguments)
    do {
      return try JSONDecoder().decode(T.self, from: data)
    } catch {
      throw RuntimeClientError.invalidOutput(String(decoding: data, as: UTF8.self))
    }
  }

  private func run(arguments: [String]) async throws -> Data {
    let executable = executableURL
    return try await Task.detached(priority: .userInitiated) {
      let process = Process()
      let stdout = Pipe()
      let stderr = Pipe()
      process.executableURL = executable
      process.arguments = arguments
      process.standardOutput = stdout
      process.standardError = stderr
      try process.run()
      let outputTask = Task.detached {
        stdout.fileHandleForReading.readDataToEndOfFile()
      }
      let errorTask = Task.detached {
        stderr.fileHandleForReading.readDataToEndOfFile()
      }
      process.waitUntilExit()
      let output = await outputTask.value
      let errorData = await errorTask.value
      if process.terminationStatus != 0 {
        let message = String(decoding: errorData, as: UTF8.self).trimmingCharacters(
          in: .whitespacesAndNewlines)
        throw RuntimeClientError.commandFailed(process.terminationStatus, message)
      }
      return output
    }.value
  }

  private func globalArguments(_ winePath: String?) -> [String] {
    guard let winePath, !winePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      return []
    }
    return ["--wine", winePath]
  }
}

private struct LineFramer {
  private var buffer = Data()

  mutating func append(_ data: Data) -> [String] {
    buffer.append(data)
    var lines: [String] = []
    while let newline = buffer.firstIndex(of: 0x0A) {
      let lineData = buffer[..<newline]
      let normalized = lineData.last == 0x0D ? lineData.dropLast() : lineData[...]
      lines.append(String(decoding: normalized, as: UTF8.self))
      buffer.removeSubrange(...newline)
    }
    return lines
  }

  mutating func finish() -> String? {
    guard !buffer.isEmpty else {
      return nil
    }
    let normalized = buffer.last == 0x0D ? buffer.dropLast() : buffer[...]
    buffer.removeAll(keepingCapacity: false)
    return String(decoding: normalized, as: UTF8.self)
  }
}
