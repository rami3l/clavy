import Cocoa

// https://juejin.cn/post/6919716600543182855
class WindowChangeObserver: NSObject {
  var currentAppPID: pid_t
  var element: AXUIElement
  var rawObserver: AXObserver?

  let notifNames =
    [
      kAXFocusedWindowChangedNotification:
        Claveilleur.focusedWindowChangedNotification,
      kAXApplicationHiddenNotification:
        Claveilleur.appHiddenNotification,
    ] as [CFString: Notification.Name]

  let observerCallbackWithInfo: AXObserverCallbackWithInfo = {
    (_, _, notif, _, refcon) in
    guard let refcon = refcon else {
      return
    }
    let slf = Unmanaged<WindowChangeObserver>.fromOpaque(refcon).takeUnretainedValue()
    log.debug("WindowChangeObserver: received \(notif) from \(slf.currentAppPID)")

    guard let notifName = slf.notifNames[notif] else {
      log.warning("WindowChangeObserver: unknown notification `\(notif)` detected")
      return
    }
    localNotificationCenter.post(
      name: notifName,
      object: slf.currentAppPID
    )
  }

  init(pid: pid_t) throws {
    currentAppPID = pid
    element = AXUIElementCreateApplication(currentAppPID)
    super.init()

    try AXObserverCreateWithInfoCallback(
      currentAppPID,
      observerCallbackWithInfo,
      &rawObserver
    )
    .unwrap()

    let selfPtr = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
    try notifNames.keys.forEach {
      try AXObserverAddNotification(rawObserver!, element, $0, selfPtr).unwrap()
    }
    CFRunLoopAddSource(
      CFRunLoopGetCurrent(),
      AXObserverGetRunLoopSource(rawObserver!),
      CFRunLoopMode.defaultMode
    )
  }

  deinit {
    CFRunLoopRemoveSource(
      CFRunLoopGetCurrent(),
      AXObserverGetRunLoopSource(rawObserver!),
      CFRunLoopMode.defaultMode
    )
    notifNames.keys.forEach {
      do {
        try AXObserverRemoveNotification(rawObserver!, element, $0).unwrap()
      } catch {}
    }
  }
}
