import ArgumentParser
import Cocoa
import Logging

@main
struct Command: AsyncParsableCommand {
  static let version = packageVersion

  static let configuration = CommandConfiguration(
    abstract: "An input source switching daemon for macOS.",
    version: version
  )

  /// The behavior flag.
  enum Operation: String, EnumerableFlag {
    case run
    case installService
    case uninstallService
    case reinstallService
    case startService
    case stopService
    case restartService
  }

  /// The common options across subcommands.
  struct Options: ParsableArguments, Decodable {
    @Flag(name: .shortAndLong, help: "Enable verbose output.")
    var verbose = false

    @Flag(exclusivity: .exclusive, help: "The operation to be performed.")
    var operation: Operation = .run
  }

  @OptionGroup var options: Options

  func run() async throws {
    var logLevel = Logger.Level.info
    if self.options.verbose {
      logLevel = .debug
    }
    LoggingSystem.bootstrap {
      var handler = StreamLogHandler.standardError(label: $0)
      handler.logLevel = logLevel
      return handler
    }

    switch self.options.operation {
    case .installService: try Service.install()
    case .uninstallService: try Service.uninstall()
    case .reinstallService: try Service.reinstall()
    case .startService: try Service.start()
    case .stopService: try Service.stop()
    case .restartService: try Service.restart()
    case .run:
      guard hasAXPrivilege() else {
        log.error("Accessibility privilege not detected, bailing out...")
        return
      }

      log.info("== Welcome to Claveilleur ==")

      // Activate the observers.
      _ = RunningAppsObserver()
      _ = await (observeCurrentInputSource(), observeAppActivation())
    }
  }
}
