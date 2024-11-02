import Cocoa
import Synchronization

actor RunningAppsObserver: NSObject {
  let currentWorkspace: NSWorkspace
  private let windowChangeObservers: Mutex<[pid_t: WindowChangeObserver?]>
  private var rawObserver: NSKeyValueObservation?

  init(workspace: NSWorkspace = .shared) {
    currentWorkspace = workspace

    windowChangeObservers =
      Mutex(
        Dictionary(
          uniqueKeysWithValues:
            Self.getWindowChangePIDs(for: workspace.runningApplications)
            .map { ($0, try? WindowChangeObserver(pid: $0)) }
        )
      )

    super.init()

    rawObserver = currentWorkspace.observe(\.runningApplications) {
      _,
      deltas in

      let oldKeys = Self.getWindowChangePIDs(for: deltas.oldValue!)
      let newKeys = Self.getWindowChangePIDs(for: deltas.newValue!)

      let toRemove = oldKeys.subtracting(newKeys)
      if !toRemove.isEmpty {
        log.debug("RunningAppsObserver: removing from windowChangeObservers: \(toRemove)")
      }

      let toAdd = newKeys.subtracting(oldKeys)
      if !toAdd.isEmpty {
        log.debug("RunningAppsObserver: adding to windowChangeObservers: \(toAdd)")
      }

      self.windowChangeObservers.withLock { observers in
        toRemove.forEach { observers.removeValue(forKey: $0) }
        toAdd.forEach { observers[$0] = try? WindowChangeObserver(pid: $0) }
      }
    }
  }

  static func getWindowChangePIDs(
    for runningApps: [NSRunningApplication]
  ) -> Set<pid_t> {
    // https://apple.stackexchange.com/a/317705
    // https://gist.github.com/ljos/3040846
    // https://stackoverflow.com/a/61688877
    let includingWindowAppPIDs =
      (CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID)! as Array)
      .compactMap { $0.object(forKey: kCGWindowOwnerPID) as? pid_t }

    // HACK: When hiding some system apps, `AXApplicationHidden` is not sent.
    // We exclude these apps from the observation for now.
    // See: https://github.com/rami3l/Claveilleur/issues/3
    let specialSystemAppIDs = Set<String?>([
      "com.apple.controlcenter",
      "com.apple.notificationcenterui",
    ])

    return Set(
      runningApps.lazy
        .filter { !specialSystemAppIDs.contains($0.bundleIdentifier) }
        .map { $0.processIdentifier }
        .filter { includingWindowAppPIDs.contains($0) }
    )
  }
}
