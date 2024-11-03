import Cocoa
import Combine

class RunningAppsObserver {
  @objc dynamic var workspace: NSWorkspace

  init(workspace: NSWorkspace = .shared) {
    self.workspace = workspace
  }

  func start() async {
    var windowChangeObservers = [pid_t: WindowChangeObserver?]()

    for await runningApps in workspace.publisher(for: \.runningApplications).values {
      let oldKeys = Set(windowChangeObservers.keys)
      let newKeys = getWindowChangePIDs(for: runningApps)

      let toRemove = oldKeys.subtracting(newKeys)
      if !toRemove.isEmpty {
        log.debug("\(#function): removing from windowChangeObservers: \(toRemove)")
      }
      toRemove.forEach { windowChangeObservers.removeValue(forKey: $0) }

      let toAdd = newKeys.subtracting(oldKeys)
      if !toAdd.isEmpty {
        log.debug("\(#function): adding to windowChangeObservers: \(toAdd)")
      }
      toAdd.forEach { windowChangeObservers[$0] = try? WindowChangeObserver(pid: $0) }
    }
  }

  func getWindowChangePIDs(for runningApps: [NSRunningApplication]) -> Set<pid_t> {
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
