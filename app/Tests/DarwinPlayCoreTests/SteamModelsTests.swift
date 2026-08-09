import DarwinPlayCore
import Foundation
import XCTest

final class SteamModelsTests: XCTestCase {
  func testDecodesSteamStatus() throws {
    let data = Data(
      #"{"installed":true,"running":true,"prefix":"/tmp/steam","steamPath":"/tmp/steam/steam.exe","gamesInstalled":2,"uiPolicyCurrent":true,"prefixRuntimeCompatible":false,"prefixRuntimeVersion":"wine-11.0"}"#
        .utf8)
    let status = try JSONDecoder().decode(SteamStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertTrue(status.running)
    XCTAssertEqual(status.gamesInstalled, 2)
    XCTAssertTrue(status.uiPolicyCurrent)
    XCTAssertFalse(status.prefixRuntimeCompatible)
    XCTAssertEqual(status.prefixRuntimeVersion, "wine-11.0")
  }

  func testDecodesSteamUiPolicyMismatch() throws {
    let data = Data(
      #"{"installed":true,"running":true,"prefix":"/tmp/steam","steamPath":"/tmp/steam/steam.exe","gamesInstalled":0,"uiPolicyCurrent":false}"#
        .utf8)
    let status = try JSONDecoder().decode(SteamStatus.self, from: data)

    XCTAssertTrue(status.running)
    XCTAssertFalse(status.uiPolicyCurrent)
  }

  func testDecodesSteamLibrary() throws {
    let data = Data(
      #"{"games":[{"appId":570,"name":"Dota 2","installDir":"dota 2 beta","installPath":"/tmp/dota","manifestPath":"/tmp/appmanifest_570.acf","stateFlags":4,"sizeOnDisk":100}]}"#
        .utf8)
    let library = try JSONDecoder().decode(SteamLibrary.self, from: data)

    XCTAssertEqual(library.games.count, 1)
    XCTAssertEqual(library.games[0].appId, 570)
    XCTAssertEqual(library.games[0].stateFlags, 4)
  }

  func testDecodesSteamCompatibilityProfile() throws {
    let data = Data(
      #"{"appId":292030,"name":"The Witcher 3","selectedExecutable":null,"launchArguments":["-dx11"],"recommendedExecutable":"bin/x64/witcher3.exe","compatibility":"promising","reasons":["Direct3D 11 imports detected"],"candidates":[{"relativePath":"bin/x64/witcher3.exe","architecture":"x86_64","subsystem":"windows-gui","graphicsApis":["Direct3D 11 / DXGI"],"score":357,"kind":"game","compatibility":"promising","reasons":["Direct3D 11 imports detected"]}]}"#
        .utf8)
    let profile = try JSONDecoder().decode(SteamCompatibilityProfile.self, from: data)

    XCTAssertEqual(profile.appId, 292030)
    XCTAssertEqual(profile.launchArguments, ["-dx11"])
    XCTAssertEqual(profile.recommendedExecutable, "bin/x64/witcher3.exe")
    XCTAssertEqual(profile.compatibility, .promising)
    XCTAssertEqual(profile.candidates.first?.relativePath, "bin/x64/witcher3.exe")
  }

  func testDecodesUnsupportedD3D12Profile() throws {
    let data = Data(
      #"{"appId":1091500,"name":"Game","selectedExecutable":"game_dx12.exe","launchArguments":[],"recommendedExecutable":"game_dx12.exe","compatibility":"unsupported","reasons":["Direct3D 12 imports detected"],"candidates":[]}"#
        .utf8)
    let profile = try JSONDecoder().decode(SteamCompatibilityProfile.self, from: data)

    XCTAssertEqual(profile.compatibility, .unsupported)
    XCTAssertEqual(profile.selectedExecutable, "game_dx12.exe")
    XCTAssertTrue(profile.candidates.isEmpty)
  }
  func testDecodesWineProbeFailure() throws {
    let data = Data(
      #"{"installed":true,"ready":false,"runtimeId":"darwinwine-10.20-dp1","runtimeName":"DarwinWine","winePath":"/tmp/darwinwine/bin/wine","wineVersion":null,"darwinWineVersion":"10.20-dp1","architecture":"x86_64","channel":"experimental","steamValidated":false,"steamLoginValidated":false,"probeError":"process failed"}"#
        .utf8)
    let status = try JSONDecoder().decode(DarwinWineStatus.self, from: data)

    XCTAssertTrue(status.installed)
    XCTAssertFalse(status.ready)
    XCTAssertEqual(status.probeError, "process failed")
  }

}
