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
    let expected = AppSettings(
      graphicsBackend: .dxmt,
      steamUiBackend: .dxmt
    )
    try await store.save(expected)
    let actual = try await store.load()

    XCTAssertEqual(actual, expected)
  }

  func testSettingsDecodeLegacyPayload() throws {
    let data = Data(#"{"winePath":"/legacy/wine"}"#.utf8)
    let settings = try JSONDecoder().decode(AppSettings.self, from: data)

    XCTAssertEqual(settings.graphicsBackend, .auto)
    XCTAssertEqual(settings.steamUiBackend, .auto)
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

  func testDarwinWineStatusDecodes() throws {
    let data = Data(
      #"{"installed":true,"ready":true,"runtimeId":"darwinwine-10.20-dp1","runtimeName":"DarwinWine","winePath":"/tmp/darwinwine/bin/wine","wineVersion":"wine-10.20","darwinWineVersion":"10.20-dp1","architecture":"x86_64","channel":"experimental","steamValidated":false,"steamLoginValidated":false,"probeError":null}"#
        .utf8)
    let status = try JSONDecoder().decode(DarwinWineStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertTrue(status.ready)
    XCTAssertEqual(status.runtimeId, "darwinwine-10.20-dp1")
    XCTAssertEqual(status.darwinWineVersion, "10.20-dp1")
    XCTAssertEqual(status.wineVersion, "wine-10.20")
    XCTAssertEqual(status.architecture, "x86_64")
  }

  func testDarwinWineStatusDecodesNotReady() throws {
    let data = Data(
      #"{"installed":true,"ready":false,"runtimeId":"darwinwine-10.20-dp1","runtimeName":"DarwinWine","winePath":"/tmp/darwinwine/bin/wine","wineVersion":null,"darwinWineVersion":"10.20-dp1","architecture":"x86_64","channel":"experimental","steamValidated":false,"steamLoginValidated":false,"probeError":"wineboot failed"}"#
        .utf8)
    let status = try JSONDecoder().decode(DarwinWineStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertFalse(status.ready)
    XCTAssertEqual(status.probeError, "wineboot failed")
  }

  func testRuntimeEventDecodes() throws {
    let data = Data(#"{"kind":"started","backend":"dxmt","pid":42,"prefix":"/tmp/prefix"}"#.utf8)
    let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)

    XCTAssertEqual(event.kind, "started")
    XCTAssertEqual(event.backend, "dxmt")
    XCTAssertEqual(event.pid, 42)
    XCTAssertEqual(event.prefix, "/tmp/prefix")
  }

  func testRuntimeProgressEventDecodes() throws {
    let data = Data(
      #"{"kind":"wine_runtime_progress","phase":"Extracting DarwinWine","message":"Downloading…","progress":0.5,"overallProgress":0.235,"currentBytes":100,"totalBytes":200}"#
        .utf8)
    let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)

    XCTAssertEqual(event.kind, "wine_runtime_progress")
    XCTAssertEqual(event.phase, "Extracting DarwinWine")
    XCTAssertEqual(event.progress, 0.5)
    XCTAssertEqual(event.overallProgress, 0.235)
    XCTAssertEqual(event.currentBytes, 100)
    XCTAssertEqual(event.totalBytes, 200)
  }

  func testDxmtStatusDecodes() throws {
    let data = Data(
      #"{"installed":true,"root":"/tmp/dxmt","mode":"builtin","sourceName":"dxmt-v0.80-builtin.tar.gz","hasD3d10core":true,"version":"v0.80","managed":true}"#
        .utf8)
    let status = try JSONDecoder().decode(DxmtStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertEqual(status.mode, .builtin)
    XCTAssertTrue(status.hasD3d10core)
    XCTAssertEqual(status.version, "v0.80")
    XCTAssertTrue(status.managed)
  }
}
