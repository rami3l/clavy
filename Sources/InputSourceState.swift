import Cocoa

let inputSourceState = InputSourceState()

actor InputSourceState {
  private var state = [String: String]()

  func load(forApp appID: String) -> String? {
    // It's a pity that we don't have `.get()` in Swift...
    return state.index(forKey: appID).map { state[$0].1 }
  }

  func save(_ id: String, forApp appID: String) {
    state[appID] = id
  }
}
