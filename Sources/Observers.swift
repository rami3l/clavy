import Carbon
import Cocoa
import Combine

// Special thanks to
// <https://stackoverflow.com/questions/36264038/cocoa-programmatically-detect-frontmost-floating-windows>
// for providing the basic methodological guidance for supporting Spotlight and co.

// https://stackoverflow.com/a/26697027
func observeCurrentInputSource() async {
  let inputSourcePublisher = DistributedNotificationCenter
    .default
    .publisher(for: kTISNotifySelectedKeyboardInputSourceChanged as Notification.Name)
    .map { _ in getInputSource() }
    .removeDuplicates()
  for await inputSource in inputSourcePublisher.values {
    guard let currentApp = getCurrentAppBundleID() else {
      log.warning(
        "\(#function): failed to get current app bundle ID for notification"
      )
      return
    }
    log.debug(
      "\(#function): updating input source for `\(currentApp)` to: \(inputSource)"
    )
    await inputSourceState.save(inputSource, forApp: currentApp)
  }
}

func observeAppActivation() async {
  let focusedWindowChangedPublisher =
    localNotificationCenter
    .publisher(for: Claveilleur.focusedWindowChangedNotification)
    .compactMap { n in getAppBundleID(forPID: n.object as! pid_t).map { (n.name, $0) } }

  let didActivateAppPublisher = NSWorkspace.shared.notificationCenter
    .publisher(for: NSWorkspace.didActivateApplicationNotification)
    .compactMap { n in getAppBundleID(forNotification: n).map { (n.name, $0) } }

  let appHiddenPublisher =
    localNotificationCenter
    .publisher(for: Claveilleur.appHiddenNotification)
    .compactMap { n in getCurrentAppBundleID().map { (n.name, $0) } }

  let currentAppPublisher =
    focusedWindowChangedPublisher
    .merge(with: didActivateAppPublisher, appHiddenPublisher)
    .removeDuplicates(by: { $0.1 == $1.1 })

  for await (notifName, currentApp) in currentAppPublisher.values {
    log.debug("\(#function): detected `\(notifName as NSString)` from: \(currentApp)")

    guard
      let oldInputSource = await inputSourceState.load(forApp: currentApp),
      setInputSource(to: oldInputSource)
    else {
      let newInputSource = getInputSource()
      log.info(
        "\(#function): registering input source for `\(currentApp)` as: \(newInputSource)"
      )
      await inputSourceState.save(newInputSource, forApp: currentApp)
      return
    }
  }
}
