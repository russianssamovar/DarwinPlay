import AppKit
import DarwinPlayCore
import Foundation
import Observation

enum ConsoleComponent: String, CaseIterable, Identifiable {
  case wine = "Wine"
  case steam = "Steam"
  case graphics = "Graphics"
  case runtime = "Runtime"

  var id: String { rawValue }
}

enum ConsoleLevel {
  case info
  case success
  case warning
  case error
}

struct ConsoleEntry: Identifiable {
  let id = UUID()
  let timestamp: Date
  let component: ConsoleComponent
  let level: ConsoleLevel
  let message: String
}

enum PlayableItem: Identifiable {
  case steam(SteamGame)
  case imported(GameRecord)

  var id: String {
    switch self {
    case .steam(let game): "steam:\(game.appId)"
    case .imported(let game): "imported:\(game.id.uuidString)"
    }
  }

  var name: String {
    switch self {
    case .steam(let game): game.name
    case .imported(let game): game.name
    }
  }
}

@MainActor
@Observable
final class AppModel {
  var games: [GameRecord] = []
  var steamGames: [SteamGame] = []
  var selection: LibrarySelection?
  var inspection: PEReport?
  var doctorReport: DoctorReport?
  var wineStatus: WineStatus?
  var dxmtStatus: DxmtStatus?
  var steamStatus: SteamStatus?
  var steamProfile: SteamCompatibilityProfile?
  var compatibilityProfiles: [UInt32: SteamCompatibilityProfile] = [:]
  var settings = AppSettings()
  var activity = LibraryActivity()
  var consoleEntries: [ConsoleEntry] = []
  var errorMessage: String?
  var isImporting = false
  var isShowingSettings = false
  var isInstallingSteam = false
  var isManagingWine = false
  var wineManagementTitle = ""
  var steamSessionRunning = false
  var steamLaunchingAppID: UInt32?
  var runningGameIDs: Set<UUID> = []

  var steamIsRunning: Bool {
    steamSessionRunning || steamStatus?.running == true
  }

  var steamUiRestartRequired: Bool {
    steamStatus?.running == true && steamStatus?.uiPolicyCurrent == false
  }

  @ObservationIgnored private var libraryStore: LibraryStore?
  @ObservationIgnored private var settingsStore: SettingsStore?
  @ObservationIgnored private var activityStore: ActivityStore?
  @ObservationIgnored private var runtimeClient: RuntimeClient?
  @ObservationIgnored private var launchTasks: [UUID: Task<Void, Never>] = [:]
  @ObservationIgnored private var steamTask: Task<Void, Never>?
  @ObservationIgnored private var wineTask: Task<Void, Never>?
  @ObservationIgnored private var compatibilityRequests: Set<UInt32> = []

  init() {
    do {
      libraryStore = try LibraryStore()
      settingsStore = try SettingsStore()
      activityStore = try ActivityStore()
      runtimeClient = try RuntimeClient()
    } catch {
      errorMessage = error.localizedDescription
    }

    Task {
      await load()
    }
  }

  var selectedGame: GameRecord? {
    guard case .imported(let id) = selection else {
      return nil
    }
    return games.first { $0.id == id }
  }

  var selectedSteamGame: SteamGame? {
    guard case .steam(let appID) = selection else {
      return nil
    }
    return steamGames.first { $0.appId == appID }
  }

  var recentItems: [PlayableItem] {
    let steam = steamGames.map { PlayableItem.steam($0) }
    let imported = games.map { PlayableItem.imported($0) }
    return (steam + imported)
      .filter { lastPlayedAt(for: $0) != nil }
      .sorted { (lastPlayedAt(for: $0) ?? .distantPast) > (lastPlayedAt(for: $1) ?? .distantPast) }
  }

  var favoriteItems: [PlayableItem] {
    let steam = steamGames.map { PlayableItem.steam($0) }
    let imported = games.map { PlayableItem.imported($0) }
    return (steam + imported)
      .filter { isFavorite($0) }
      .sorted {
        let lhs = lastPlayedAt(for: $0) ?? .distantPast
        let rhs = lastPlayedAt(for: $1) ?? .distantPast
        if lhs != rhs { return lhs > rhs }
        return $0.name.localizedStandardCompare($1.name) == .orderedAscending
      }
  }

  var latestItem: PlayableItem? {
    recentItems.first
  }

  func load() async {
    do {
      if let libraryStore {
        games = try await libraryStore.load()
      }
      if let settingsStore {
        settings = try await settingsStore.load()
      }
      if let activityStore {
        activity = try await activityStore.load()
      }
      await refreshWine()
      await refreshDxmtStatus()
      await refreshSteam()
      if selection == nil {
        selection = .home
      }
      await refreshInspection()
      await refreshSteamProfile()
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func select(_ value: LibrarySelection?) {
    selection = value
    Task {
      await refreshInspection()
      await refreshSteamProfile()
    }
  }

  func select(_ item: PlayableItem) {
    switch item {
    case .steam(let game): select(.steam(game.appId))
    case .imported(let game): select(.imported(game.id))
    }
  }

  func importGame(_ url: URL) async {
    guard url.pathExtension.lowercased() == "exe" else {
      errorMessage = "Select a Windows .exe file"
      return
    }

    do {
      guard let runtimeClient else {
        throw RuntimeClientError.runtimeNotFound
      }
      let report = try await runtimeClient.inspect(executable: url)
      let baseName = url.deletingPathExtension().lastPathComponent
      let game = GameRecord(name: baseName, executablePath: url.path)
      games.append(game)
      try await libraryStore?.save(games)
      selection = .imported(game.id)
      inspection = report
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func removeSelectedGame() async {
    guard let game = selectedGame, !runningGameIDs.contains(game.id) else {
      return
    }

    games.removeAll { $0.id == game.id }
    activity.entries.removeValue(forKey: importedActivityKey(game.id))
    do {
      try await libraryStore?.save(games)
      try await activityStore?.save(activity)
      selection = .games
      await refreshInspection()
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func launchSelectedGame() {
    guard let game = selectedGame,
      let runtimeClient,
      !runningGameIDs.contains(game.id)
    else {
      return
    }

    runningGameIDs.insert(game.id)
    recordPlayed(.imported(game))
    appendConsole(.wine, .info, "Launching \(game.name)")
    let winePath = normalizedWinePath
    let backend = settings.graphicsBackend

    let task = Task {
      defer {
        runningGameIDs.remove(game.id)
        launchTasks.removeValue(forKey: game.id)
      }

      do {
        for try await event in runtimeClient.launch(
          game: game, winePath: winePath, backend: backend)
        {
          consume(event, component: .wine)
        }
      } catch {
        appendConsole(.wine, .error, error.localizedDescription)
        errorMessage = error.localizedDescription
      }
    }
    launchTasks[game.id] = task
  }

  func stopSelectedGame() async {
    guard let game = selectedGame, let runtimeClient else {
      return
    }

    do {
      try await runtimeClient.stop(gameID: game.id, winePath: normalizedWinePath)
      appendConsole(.wine, .info, "Stopped \(game.name)")
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func resetSelectedPrefix() async {
    guard let game = selectedGame,
      let runtimeClient,
      !runningGameIDs.contains(game.id)
    else {
      return
    }

    do {
      try await runtimeClient.resetPrefix(gameID: game.id, winePath: normalizedWinePath)
      appendConsole(.wine, .success, "Reset prefix for \(game.name)")
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func installWine() {
    guard let runtimeClient, !isManagingWine else { return }
    beginWineManagement("Installing Wine", stream: runtimeClient.installWine())
  }

  func reinstallWine() {
    guard let runtimeClient, !isManagingWine else { return }
    beginWineManagement("Reinstalling Wine", stream: runtimeClient.reinstallWine())
  }

  func removeWine() {
    guard let runtimeClient, !isManagingWine else { return }
    beginWineManagement("Removing Wine", stream: runtimeClient.removeWine())
  }

  func openHomebrewWebsite() {
    guard let url = URL(string: "https://brew.sh") else { return }
    NSWorkspace.shared.open(url)
  }

  func openWineApplication() {
    guard let path = wineStatus?.winePath, let range = path.range(of: ".app") else { return }
    let applicationPath = String(path[..<range.upperBound])
    NSWorkspace.shared.open(URL(fileURLWithPath: applicationPath))
  }

  func openPrivacyAndSecurity() {
    if let url = URL(
      string: "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension")
    {
      NSWorkspace.shared.open(url)
    }
  }

  func retryWineProbe() {
    Task { await refreshWine() }
  }

  func installSteam() async {
    guard let runtimeClient, !isInstallingSteam, wineStatus?.ready == true else {
      return
    }
    isInstallingSteam = true
    appendConsole(.steam, .info, "Installing the Windows Steam client")
    defer { isInstallingSteam = false }
    do {
      steamStatus = try await runtimeClient.installSteam(winePath: normalizedWinePath)
      appendConsole(.steam, .success, "Steam installation completed")
      await refreshSteam()
    } catch {
      appendConsole(.steam, .error, error.localizedDescription)
      errorMessage = error.localizedDescription
    }
  }

  func refreshSteam() async {
    guard let runtimeClient else {
      return
    }
    do {
      let status = try await runtimeClient.steamStatus(winePath: normalizedWinePath)
      steamStatus = status
      if status.installed {
        let currentGames = try await runtimeClient.steamGames().games.sorted {
          $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
        steamGames = currentGames
        let installed = Set(currentGames.map(\.appId))
        compatibilityProfiles = compatibilityProfiles.filter { installed.contains($0.key) }
        await refreshSteamProfile()
      } else {
        steamGames = []
        compatibilityProfiles = [:]
        steamProfile = nil
        if case .steam = selection {
          selection = .games
        }
      }
    } catch {
      steamGames = []
      steamProfile = nil
      errorMessage = error.localizedDescription
    }
  }

  func ensureCompatibility(for game: SteamGame) async {
    guard compatibilityProfiles[game.appId] == nil,
      !compatibilityRequests.contains(game.appId),
      let runtimeClient
    else {
      return
    }
    compatibilityRequests.insert(game.appId)
    defer { compatibilityRequests.remove(game.appId) }
    do {
      let profile = try await runtimeClient.steamProfile(
        appID: game.appId,
        fallbackBackend: settings.graphicsBackend
      )
      compatibilityProfiles[game.appId] = profile
      if selectedSteamGame?.appId == game.appId {
        steamProfile = profile
      }
    } catch {
      appendConsole(
        .runtime, .warning,
        "Compatibility analysis failed for \(game.name): \(error.localizedDescription)")
    }
  }

  func refreshSteamProfile() async {
    guard let game = selectedSteamGame, let runtimeClient else {
      steamProfile = nil
      return
    }
    do {
      let profile = try await runtimeClient.steamProfile(
        appID: game.appId,
        fallbackBackend: settings.graphicsBackend
      )
      if selectedSteamGame?.appId == game.appId {
        steamProfile = profile
        compatibilityProfiles[game.appId] = profile
      }
    } catch {
      if selectedSteamGame?.appId == game.appId {
        steamProfile = nil
      }
      errorMessage = error.localizedDescription
    }
  }

  func saveSteamProfile(
    backend: SteamBackendOverride,
    executable: String?,
    launchArguments: [String]
  ) async {
    guard let game = selectedSteamGame, let runtimeClient else {
      return
    }
    do {
      let profile = try await runtimeClient.saveSteamProfile(
        appID: game.appId,
        backend: backend,
        executable: executable,
        launchArguments: launchArguments,
        fallbackBackend: settings.graphicsBackend
      )
      steamProfile = profile
      compatibilityProfiles[game.appId] = profile
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func resetSteamProfile() async {
    guard let game = selectedSteamGame, let runtimeClient else {
      return
    }
    do {
      let profile = try await runtimeClient.resetSteamProfile(
        appID: game.appId,
        fallbackBackend: settings.graphicsBackend
      )
      steamProfile = profile
      compatibilityProfiles[game.appId] = profile
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func openSteam() {
    guard let runtimeClient, steamStatus?.installed == true else {
      return
    }
    if steamIsRunning {
      appendConsole(.steam, .info, "Steam is already running")
      return
    }
    steamLaunchingAppID = nil
    appendConsole(.steam, .info, "Opening Steam")
    beginSteamSession(runtimeClient.startSteam(winePath: normalizedWinePath))
  }

  func restartSteamUI() {
    guard let runtimeClient, steamStatus?.installed == true else {
      return
    }
    steamLaunchingAppID = nil
    appendConsole(.steam, .info, "Restarting Steam UI with software web rendering")
    beginSteamSession(runtimeClient.restartSteam(winePath: normalizedWinePath))
  }

  func launchSelectedSteamGame() {
    guard let game = selectedSteamGame, let runtimeClient else {
      return
    }
    recordPlayed(.steam(game))
    steamLaunchingAppID = game.appId
    appendConsole(
      .steam, .info,
      steamIsRunning
        ? "Launching \(game.name) through the running Steam client"
        : "Launching \(game.name) through Steam"
    )
    beginSteamSession(
      runtimeClient.launchSteamGame(
        appID: game.appId,
        winePath: normalizedWinePath,
        backend: settings.graphicsBackend
      ))
  }

  func launch(_ item: PlayableItem) {
    select(item)
    switch item {
    case .steam:
      launchSelectedSteamGame()
    case .imported:
      launchSelectedGame()
    }
  }

  func stopSteam() async {
    guard let runtimeClient else {
      return
    }
    do {
      try await runtimeClient.stopSteam(winePath: normalizedWinePath)
      steamTask?.cancel()
      steamTask = nil
      steamSessionRunning = false
      steamLaunchingAppID = nil
      await refreshSteam()
      appendConsole(.steam, .info, "Steam stopped")
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func resetSteam() async {
    guard let runtimeClient, !steamIsRunning else {
      return
    }
    do {
      try await runtimeClient.resetSteam(winePath: normalizedWinePath)
      steamGames = []
      compatibilityProfiles = [:]
      steamStatus = try await runtimeClient.steamStatus(winePath: normalizedWinePath)
      if case .steam = selection {
        selection = .games
      }
      appendConsole(.steam, .success, "Steam prefix reset")
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func refreshWine() async {
    guard let runtimeClient else { return }
    do {
      let status = try await runtimeClient.wineStatus(winePath: normalizedWinePath)
      wineStatus = status
      if status.ready {
        doctorReport = try? await runtimeClient.doctor(winePath: normalizedWinePath)
      } else {
        doctorReport = nil
      }
    } catch {
      wineStatus = nil
      doctorReport = nil
      errorMessage = error.localizedDescription
    }
  }

  func refreshDoctor() async {
    await refreshWine()
  }

  func refreshDxmtStatus() async {
    guard let runtimeClient else {
      return
    }

    do {
      dxmtStatus = try await runtimeClient.dxmtStatus()
    } catch {
      dxmtStatus = nil
      errorMessage = error.localizedDescription
    }
  }

  func installDxmt(source: URL, mode: DxmtMode) async {
    guard let runtimeClient else {
      return
    }

    do {
      dxmtStatus = try await runtimeClient.installDxmt(source: source, mode: mode)
      compatibilityProfiles = [:]
      appendConsole(.graphics, .success, "DXMT installed")
      await refreshSteamProfile()
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func removeDxmt() async {
    guard let runtimeClient else {
      return
    }

    do {
      try await runtimeClient.removeDxmt()
      dxmtStatus = try await runtimeClient.dxmtStatus()
      compatibilityProfiles = [:]
      appendConsole(.graphics, .info, "DXMT removed")
      await refreshSteamProfile()
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func refreshInspection() async {
    guard let game = selectedGame, let runtimeClient else {
      inspection = nil
      return
    }

    do {
      inspection = try await runtimeClient.inspect(
        executable: URL(fileURLWithPath: game.executablePath))
    } catch {
      inspection = nil
      errorMessage = error.localizedDescription
    }
  }

  func saveSettings(_ value: AppSettings) async {
    do {
      settings = value
      try await settingsStore?.save(value)
      isShowingSettings = false
      compatibilityProfiles = [:]
      await refreshWine()
      await refreshSteamProfile()
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  func isFavorite(_ item: PlayableItem) -> Bool {
    activity.entries[item.id]?.isFavorite == true
  }

  func toggleFavorite(_ item: PlayableItem) {
    var entry = activity.entries[item.id] ?? LibraryActivityEntry()
    entry.isFavorite.toggle()
    activity.entries[item.id] = entry
    Task { try? await activityStore?.save(activity) }
  }

  func lastPlayedAt(for item: PlayableItem) -> Date? {
    activity.entries[item.id]?.lastPlayedAt
  }

  func lastPlayedText(for item: PlayableItem) -> String {
    guard let date = lastPlayedAt(for: item) else { return "Never played" }
    return date.formatted(.relative(presentation: .named))
  }

  func clearConsole() {
    consoleEntries.removeAll(keepingCapacity: true)
  }

  private var normalizedWinePath: String? {
    let value = settings.winePath.trimmingCharacters(in: .whitespacesAndNewlines)
    return value.isEmpty ? nil : value
  }

  private func importedActivityKey(_ id: UUID) -> String {
    "imported:\(id.uuidString)"
  }

  private func recordPlayed(_ item: PlayableItem) {
    var entry = activity.entries[item.id] ?? LibraryActivityEntry()
    entry.lastPlayedAt = .now
    activity.entries[item.id] = entry
    Task { try? await activityStore?.save(activity) }
  }

  private func beginSteamSession(_ stream: AsyncThrowingStream<RuntimeEvent, Error>) {
    steamSessionRunning = true
    steamTask = Task {
      defer {
        steamSessionRunning = false
        steamLaunchingAppID = nil
        steamTask = nil
        Task { await refreshSteam() }
      }
      do {
        for try await event in stream {
          consume(event, component: .steam)
        }
      } catch is CancellationError {
      } catch {
        appendConsole(.steam, .error, error.localizedDescription)
        errorMessage = error.localizedDescription
      }
    }
  }

  private func beginWineManagement(
    _ title: String,
    stream: AsyncThrowingStream<RuntimeEvent, Error>
  ) {
    isManagingWine = true
    wineManagementTitle = title
    appendConsole(.wine, .info, title)
    wineTask = Task {
      defer {
        isManagingWine = false
        wineManagementTitle = ""
        wineTask = nil
        Task {
          await refreshWine()
          await refreshSteam()
        }
      }
      do {
        for try await event in stream {
          consume(event, component: .wine)
        }
        appendConsole(.wine, .success, "\(title) completed")
      } catch is CancellationError {
      } catch {
        appendConsole(.wine, .error, error.localizedDescription)
        errorMessage = error.localizedDescription
      }
    }
  }

  private func consume(_ event: RuntimeEvent, component: ConsoleComponent) {
    switch event.kind {
    case "started":
      if let backend = event.backend {
        appendConsole(component, .success, "Process started using \(backend)")
        appendConsole(.graphics, .info, "Active backend: \(backend)")
      } else if let pid = event.pid {
        appendConsole(component, .success, "Process started · PID \(pid)")
      }
    case "already_running", "reusing_running":
      appendConsole(component, .info, event.message ?? "Steam is already running")
    case "restarting_ui":
      appendConsole(component, .info, event.message ?? "Restarting Steam UI")
    case "log":
      let level: ConsoleLevel = event.stream == "stderr" ? .warning : .info
      appendConsole(component, level, event.message ?? "")
    case "exited":
      let code = event.exitCode ?? -1
      if component == .steam, code == 42 {
        appendConsole(.steam, .info, "Steam client updated · restarting")
      } else {
        appendConsole(
          component, code == 0 ? .success : .warning, "Process exited with code \(code)")
      }
    default:
      appendConsole(component, .info, event.message ?? event.kind)
    }
  }

  private func appendConsole(
    _ component: ConsoleComponent,
    _ level: ConsoleLevel,
    _ message: String
  ) {
    consoleEntries.append(
      ConsoleEntry(timestamp: .now, component: component, level: level, message: message))
    if consoleEntries.count > 5000 {
      consoleEntries.removeFirst(consoleEntries.count - 5000)
    }
  }
}
