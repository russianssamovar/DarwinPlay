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

  public func runtimeStatus() async throws -> DarwinWineStatus {
    try await runJSON(arguments: ["runtime", "status", "--json"])
  }

  public func installDarwinWine(archive: URL) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["runtime", "install", "--archive", archive.path, "--json"])
  }

  public func installLatestDarwinWine() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["runtime", "install-latest", "--json"])
  }

  public func removeDarwinWine() async throws {
    _ = try await run(arguments: ["runtime", "remove", "--json"])
  }

  public func doctor() async throws -> DoctorReport {
    try await runJSON(arguments: ["doctor", "--json"])
  }

  public func inspect(executable: URL) async throws -> PEReport {
    try await runJSON(arguments: ["inspect", executable.path, "--json"])
  }

  public func steamStatus() async throws -> SteamStatus {
    try await runJSON(arguments: ["steam", "status", "--json"])
  }

  public func installSteam(
    installer: URL? = nil
  ) -> AsyncThrowingStream<RuntimeEvent, Error> {
    var arguments = ["steam", "install", "--json"]
    if let installer {
      arguments += ["--installer", installer.path]
    }
    return stream(arguments: arguments)
  }

  public func steamGames() async throws -> SteamLibrary {
    try await runJSON(arguments: ["steam", "games", "--json"])
  }

  public func steamProfile(appID: UInt32) async throws -> SteamCompatibilityProfile {
    try await runJSON(arguments: [
      "steam", "profile", "show",
      "--app-id", String(appID),
      "--json",
    ])
  }

  public func saveSteamProfile(
    appID: UInt32,
    executable: String?,
    launchArguments: [String]
  ) async throws -> SteamCompatibilityProfile {
    var arguments = [
      "steam", "profile", "set",
      "--app-id", String(appID),
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

  public func resetSteamProfile(appID: UInt32) async throws -> SteamCompatibilityProfile {
    try await runJSON(arguments: [
      "steam", "profile", "reset",
      "--app-id", String(appID),
      "--json",
    ])
  }

  public func startSteam() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["steam", "start", "--json"])
  }

  public func restartSteam() -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(arguments: ["steam", "restart", "--json"])
  }

  public func launchSteamGame(appID: UInt32) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: [
        "steam", "run",
        "--app-id", String(appID),
        "--json",
      ])
  }

  public func stopSteam() async throws {
    _ = try await run(arguments: ["steam", "stop"])
  }

  public func resetSteam() async throws {
    _ = try await run(arguments: ["steam", "reset"])
  }

  public func resetPrefix(gameID: UUID) async throws {
    _ = try await run(arguments: ["prefix", "reset", "--game-id", gameID.uuidString])
  }

  public func stop(gameID: UUID) async throws {
    _ = try await run(arguments: ["stop", "--game-id", gameID.uuidString])
  }

  public func launch(game: GameRecord) -> AsyncThrowingStream<RuntimeEvent, Error> {
    stream(
      arguments: [
        "launch",
        "--game-id", game.id.uuidString,
        "--executable", game.executablePath,
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
