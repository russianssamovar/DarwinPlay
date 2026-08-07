import Foundation

public actor LibraryStore {
  private let fileURL: URL
  private let decoder = JSONDecoder()
  private let legacyDecoder = JSONDecoder()
  private let encoder = JSONEncoder()

  public init(root: URL? = nil) throws {
    let base = try root ?? Self.defaultRoot()
    try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
    fileURL = base.appendingPathComponent("library.json")
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    encoder.dateEncodingStrategy = .deferredToDate
    decoder.dateDecodingStrategy = .deferredToDate
    legacyDecoder.dateDecodingStrategy = .iso8601
  }

  public func load() throws -> [GameRecord] {
    guard FileManager.default.fileExists(atPath: fileURL.path) else {
      return []
    }
    let data = try Data(contentsOf: fileURL)
    do {
      return try decoder.decode([GameRecord].self, from: data)
    } catch {
      return try legacyDecoder.decode([GameRecord].self, from: data)
    }
  }

  public func save(_ games: [GameRecord]) throws {
    try encoder.encode(games).write(to: fileURL, options: .atomic)
  }

  private static func defaultRoot() throws -> URL {
    let root = try FileManager.default.url(
      for: .applicationSupportDirectory,
      in: .userDomainMask,
      appropriateFor: nil,
      create: true
    )
    return root.appendingPathComponent("DarwinPlay", isDirectory: true)
  }
}

public actor SettingsStore {
  private let fileURL: URL
  private let decoder = JSONDecoder()
  private let encoder = JSONEncoder()

  public init(root: URL? = nil) throws {
    let base = try root ?? Self.defaultRoot()
    try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
    fileURL = base.appendingPathComponent("settings.json")
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
  }

  public func load() throws -> AppSettings {
    guard FileManager.default.fileExists(atPath: fileURL.path) else {
      return AppSettings()
    }
    return try decoder.decode(AppSettings.self, from: Data(contentsOf: fileURL))
  }

  public func save(_ settings: AppSettings) throws {
    try encoder.encode(settings).write(to: fileURL, options: .atomic)
  }

  private static func defaultRoot() throws -> URL {
    let root = try FileManager.default.url(
      for: .applicationSupportDirectory,
      in: .userDomainMask,
      appropriateFor: nil,
      create: true
    )
    return root.appendingPathComponent("DarwinPlay", isDirectory: true)
  }
}

public actor ActivityStore {
  private let fileURL: URL
  private let decoder = JSONDecoder()
  private let encoder = JSONEncoder()

  public init(root: URL? = nil) throws {
    let base = try root ?? Self.defaultRoot()
    try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
    fileURL = base.appendingPathComponent("activity.json")
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    encoder.dateEncodingStrategy = .deferredToDate
    decoder.dateDecodingStrategy = .deferredToDate
  }

  public func load() throws -> LibraryActivity {
    guard FileManager.default.fileExists(atPath: fileURL.path) else {
      return LibraryActivity()
    }
    return try decoder.decode(LibraryActivity.self, from: Data(contentsOf: fileURL))
  }

  public func save(_ activity: LibraryActivity) throws {
    try encoder.encode(activity).write(to: fileURL, options: .atomic)
  }

  private static func defaultRoot() throws -> URL {
    let root = try FileManager.default.url(
      for: .applicationSupportDirectory,
      in: .userDomainMask,
      appropriateFor: nil,
      create: true
    )
    return root.appendingPathComponent("DarwinPlay", isDirectory: true)
  }
}
