//
//  MigrationManager.swift
//  MullvadVPN
//
//  Created by Marco Nikic on 2023-08-08.
//  Copyright © 2025 Mullvad VPN AB. All rights reserved.
//

import Foundation
import MullvadLogging
import MullvadTypes

public enum SettingsMigrationResult: Sendable {
    /// Nothing to migrate.
    case nothing

    /// Successfully performed migration.
    case success

    /// Failure when migrating store.
    case failure(Error)
}

public struct MigrationManager {
    private let logger = Logger(label: "MigrationManager")
    private let cacheDirectory: URL

    public init(cacheDirectory: URL) {
        self.cacheDirectory = cacheDirectory.appendingPathComponent("migrationState.json")
    }

    /// Migrate settings store if needed.
    ///
    /// Reads the current settings, upgrades them to the latest version if needed
    /// and writes back to `store` when settings are updated.
    ///
    /// In order to avoid migration happening from both the VPN and the host processes at the same time,
    /// a non existent file path is used as a lock to synchronize access between the processes.
    /// This file is accessed by `NSFileCoordinator` in order to prevent multiple processes accessing at the same time.
    /// - Parameters:
    ///   - store: The store to from which settings are read and written to.
    ///   - migrationCompleted: Completion handler called with a migration result.
    public func migrateSettings(
        store: SettingsStore,
        migrationCompleted: @escaping @Sendable (SettingsMigrationResult) -> Void
    ) {
        let fileCoordinator = NSFileCoordinator(filePresenter: nil)
        var error: NSError?

        // This will block the calling thread if another process is currently running the same code.
        // This is intentional to avoid TOCTOU issues, and guaranteeing settings cannot be read
        // in a half written state.
        // The resulting effect is that only one process at a time can do settings migrations.
        // The other process will be blocked, and will have nothing to do as long as settings were successfully upgraded.
        fileCoordinator.coordinate(writingItemAt: cacheDirectory, error: &error) { _ in
            let resetStoreHandler = { (result: SettingsMigrationResult) in
                // Reset store upon failure to migrate settings.
                if case .failure = result {
                    SettingsManager.resetStore()
                }
                migrationCompleted(result)
            }

            do {
                try upgradeSettingsToLatestVersion(
                    store: store,
                    migrationCompleted: migrationCompleted
                )
            } catch .itemNotFound as KeychainError {
                migrationCompleted(.nothing)
            } catch let couldNotReadKeychainError as KeychainError
                where couldNotReadKeychainError == .interactionNotAllowed {
                migrationCompleted(.failure(couldNotReadKeychainError))
            } catch {
                resetStoreHandler(.failure(error))
            }
        }
    }

    private func upgradeSettingsToLatestVersion(
        store: SettingsStore,
        migrationCompleted: @escaping @Sendable (SettingsMigrationResult) -> Void
    ) throws {
        let parser = SettingsParser(decoder: JSONDecoder(), encoder: JSONEncoder())
        let settingsData = try store.read(key: SettingsKey.settings)
        let settingsVersion = try parser.parseVersion(data: settingsData)

        guard settingsVersion != SchemaVersion.current.rawValue else {
            migrationCompleted(.nothing)
            return
        }

        // Corrupted settings version (i.e. negative values, or downgrade from a future version) should fail
        guard var savedSchema = SchemaVersion(rawValue: settingsVersion) else {
            migrationCompleted(.failure(UnsupportedSettingsVersionError(
                storedVersion: settingsVersion,
                currentVersion: SchemaVersion.current
            )))
            return
        }

        var savedSettings = try parser.parsePayload(as: savedSchema.settingsType, from: settingsData)

        repeat {
            let upgradedVersion = savedSettings.upgradeToNextVersion()
            savedSchema = savedSchema.nextVersion
            savedSettings = upgradedVersion
        } while savedSchema.rawValue < SchemaVersion.current.rawValue

        // Write the latest settings back to the store
        let latestVersionPayload = try parser.producePayload(savedSettings, version: SchemaVersion.current.rawValue)
        try store.write(latestVersionPayload, for: .settings)
        migrationCompleted(.success)
    }
}
