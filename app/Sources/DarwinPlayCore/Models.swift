import Foundation

public struct GameRecord: Codable, Identifiable, Hashable, Sendable {
  public let id: UUID
  public var name: String
  public var executablePath: String
  public let addedAt: Date

  public init(id: UUID = UUID(), name: String, executablePath: String, addedAt: Date = .now) {
    self.id = id
    self.name = name
    self.executablePath = executablePath
    self.addedAt = addedAt
  }
}

public struct SteamGame: Codable, Identifiable, Hashable, Sendable {
  public let appId: UInt32
  public let name: String
  public let installDir: String
  public let installPath: String
  public let manifestPath: String
  public let stateFlags: UInt64
  public let sizeOnDisk: UInt64

  public var id: UInt32 { appId }
}

public struct SteamLibrary: Codable, Equatable, Sendable {
  public let games: [SteamGame]
}

public struct WineStatus: Codable, Equatable, Sendable {
  public let installed: Bool
  public let ready: Bool
  public let winePath: String?
  public let wineVersion: String?
  public let probeError: String?
  public let homebrewInstalled: Bool
  public let homebrewPath: String?
  public let managedByHomebrew: Bool
}

public struct SteamStatus: Codable, Equatable, Sendable {
  public let installed: Bool
  public let running: Bool
  public let prefix: String
  public let steamPath: String?
  public let gamesInstalled: Int
  public let uiPolicyCurrent: Bool

  public init(
    installed: Bool,
    running: Bool = false,
    prefix: String,
    steamPath: String?,
    gamesInstalled: Int,
    uiPolicyCurrent: Bool = true
  ) {
    self.installed = installed
    self.running = running
    self.prefix = prefix
    self.steamPath = steamPath
    self.gamesInstalled = gamesInstalled
    self.uiPolicyCurrent = uiPolicyCurrent
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    installed = try container.decode(Bool.self, forKey: .installed)
    running = try container.decodeIfPresent(Bool.self, forKey: .running) ?? false
    prefix = try container.decode(String.self, forKey: .prefix)
    steamPath = try container.decodeIfPresent(String.self, forKey: .steamPath)
    gamesInstalled = try container.decode(Int.self, forKey: .gamesInstalled)
    uiPolicyCurrent = try container.decodeIfPresent(Bool.self, forKey: .uiPolicyCurrent) ?? true
  }
}

public enum SteamBackendOverride: String, Codable, CaseIterable, Identifiable, Sendable {
  case inherit
  case auto
  case dxmt
  case wined3d

  public var id: String { rawValue }

  public var displayName: String {
    switch self {
    case .inherit: "Use global default"
    case .auto: "Automatic for this game"
    case .dxmt: "DXMT"
    case .wined3d: "WineD3D"
    }
  }
}

public enum SteamCompatibilityLevel: String, Codable, Sendable {
  case promising
  case needsComponent = "needs-component"
  case fallback
  case unsupported
  case unknown

  public var displayName: String {
    switch self {
    case .promising: "Promising"
    case .needsComponent: "Needs component"
    case .fallback: "Fallback path"
    case .unsupported: "Unsupported"
    case .unknown: "Unknown"
    }
  }
}

public enum SteamExecutableKind: String, Codable, Sendable {
  case game
  case launcher
  case tool
  case redistributable
}

public struct SteamExecutableCandidate: Codable, Identifiable, Hashable, Sendable {
  public let relativePath: String
  public let architecture: String
  public let subsystem: String
  public let graphicsApis: [String]
  public let score: Int
  public let kind: SteamExecutableKind
  public let recommendedBackend: GraphicsBackendPreference?
  public let compatibility: SteamCompatibilityLevel
  public let reasons: [String]

  public var id: String { relativePath }
}

public struct SteamCompatibilityProfile: Codable, Equatable, Sendable {
  public let appId: UInt32
  public let name: String
  public let backendOverride: SteamBackendOverride
  public let selectedExecutable: String?
  public let launchArguments: [String]
  public let recommendedExecutable: String?
  public let recommendedBackend: GraphicsBackendPreference?
  public let effectiveBackend: GraphicsBackendPreference
  public let compatibility: SteamCompatibilityLevel
  public let reasons: [String]
  public let candidates: [SteamExecutableCandidate]
  public let dxmtInstalled: Bool
}

public enum LibrarySelection: Hashable, Sendable {
  case home
  case games
  case console
  case steamLibrary
  case importedLibrary
  case imported(UUID)
  case steam(UInt32)
}

public struct LibraryActivityEntry: Codable, Equatable, Sendable {
  public var isFavorite: Bool
  public var lastPlayedAt: Date?

  public init(isFavorite: Bool = false, lastPlayedAt: Date? = nil) {
    self.isFavorite = isFavorite
    self.lastPlayedAt = lastPlayedAt
  }
}

public struct LibraryActivity: Codable, Equatable, Sendable {
  public var entries: [String: LibraryActivityEntry]

  public init(entries: [String: LibraryActivityEntry] = [:]) {
    self.entries = entries
  }
}

public enum GraphicsBackendPreference: String, Codable, CaseIterable, Identifiable, Sendable {
  case auto
  case dxmt
  case wined3d

  public var id: String { rawValue }

  public var displayName: String {
    switch self {
    case .auto: "Automatic"
    case .dxmt: "DXMT"
    case .wined3d: "WineD3D"
    }
  }
}

public enum DxmtMode: String, Codable, CaseIterable, Identifiable, Sendable {
  case builtin
  case native

  public var id: String { rawValue }

  public var displayName: String {
    switch self {
    case .builtin: "Wine built-in"
    case .native: "Native DLL"
    }
  }
}

public struct AppSettings: Codable, Equatable, Sendable {
  public var winePath: String
  public var graphicsBackend: GraphicsBackendPreference

  public init(winePath: String = "", graphicsBackend: GraphicsBackendPreference = .auto) {
    self.winePath = winePath
    self.graphicsBackend = graphicsBackend
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    winePath = try container.decodeIfPresent(String.self, forKey: .winePath) ?? ""
    graphicsBackend =
      try container.decodeIfPresent(GraphicsBackendPreference.self, forKey: .graphicsBackend)
      ?? .auto
  }
}

public struct PEReport: Codable, Equatable, Sendable {
  public let path: String
  public let architecture: String
  public let subsystem: String
  public let entryPoint: UInt32
  public let imageBase: UInt64
  public let imports: [String]
  public let graphicsApis: [String]
}

public struct DoctorReport: Codable, Equatable, Sendable {
  public let winePath: String
  public let wineVersion: String
  public let hostArchitecture: String
  public let wineArchitecture: String
}

public struct DxmtStatus: Codable, Equatable, Sendable {
  public let installed: Bool
  public let root: String?
  public let mode: DxmtMode?
  public let sourceName: String?
  public let hasD3d10core: Bool
}

public struct RuntimeEvent: Codable, Equatable, Sendable {
  public let kind: String
  public let stream: String?
  public let message: String?
  public let backend: String?
  public let pid: UInt32?
  public let exitCode: Int32?
  public let prefix: String?
}
