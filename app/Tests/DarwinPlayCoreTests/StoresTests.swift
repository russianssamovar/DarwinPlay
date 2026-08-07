import Foundation
import XCTest

@testable import DarwinPlayCore

final class StoresTests: XCTestCase {
  func testLibraryRoundTrip() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }

    let store = try LibraryStore(root: root)
    let expected = [GameRecord(name: "Example", executablePath: "/tmp/example.exe")]
    try await store.save(expected)
    let actual = try await store.load()

    XCTAssertEqual(actual, expected)
  }

  func testSettingsRoundTrip() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }

    let store = try SettingsStore(root: root)
    let expected = AppSettings(winePath: "/opt/homebrew/bin/wine", graphicsBackend: .dxmt)
    try await store.save(expected)
    let actual = try await store.load()

    XCTAssertEqual(actual, expected)
  }

  func testSettingsDecodeLegacyPayload() throws {
    let data = Data(#"{"winePath":"/opt/homebrew/bin/wine"}"#.utf8)
    let settings = try JSONDecoder().decode(AppSettings.self, from: data)

    XCTAssertEqual(settings.graphicsBackend, .auto)
  }

  func testActivityRoundTrip() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }

    let store = try ActivityStore(root: root)
    let date = Date(timeIntervalSince1970: 1_700_000_000)
    let expected = LibraryActivity(entries: [
      "steam:570": LibraryActivityEntry(isFavorite: true, lastPlayedAt: date)
    ])
    try await store.save(expected)
    let actual = try await store.load()

    XCTAssertEqual(actual, expected)
  }

  func testWineStatusDecodes() throws {
    let data = Data(
      #"{"installed":true,"ready":true,"winePath":"/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine","wineVersion":"wine-11.0","homebrewInstalled":true,"homebrewPath":"/opt/homebrew/bin/brew","managedByHomebrew":true}"#
        .utf8)
    let status = try JSONDecoder().decode(WineStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertTrue(status.ready)
    XCTAssertTrue(status.homebrewInstalled)
    XCTAssertTrue(status.managedByHomebrew)
    XCTAssertEqual(status.wineVersion, "wine-11.0")
  }

  func testWineStatusDecodesApprovalState() throws {
    let data = Data(
      #"{"installed":true,"ready":false,"winePath":"/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine","wineVersion":null,"homebrewInstalled":true,"homebrewPath":"/opt/homebrew/bin/brew","managedByHomebrew":true}"#
        .utf8)
    let status = try JSONDecoder().decode(WineStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertFalse(status.ready)
    XCTAssertNil(status.wineVersion)
  }

  func testRuntimeEventDecodes() throws {
    let data = Data(#"{"kind":"started","backend":"dxmt","pid":42,"prefix":"/tmp/prefix"}"#.utf8)
    let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)

    XCTAssertEqual(event.kind, "started")
    XCTAssertEqual(event.backend, "dxmt")
    XCTAssertEqual(event.pid, 42)
    XCTAssertEqual(event.prefix, "/tmp/prefix")
  }

  func testDxmtStatusDecodes() throws {
    let data = Data(
      #"{"installed":true,"root":"/tmp/dxmt","mode":"builtin","sourceName":"artifact","hasD3d10core":true}"#
        .utf8)
    let status = try JSONDecoder().decode(DxmtStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertEqual(status.mode, .builtin)
    XCTAssertTrue(status.hasD3d10core)
  }
}
