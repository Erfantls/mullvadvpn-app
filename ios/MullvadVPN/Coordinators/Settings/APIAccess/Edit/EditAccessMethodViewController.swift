//
//  EditAccessMethodViewController.swift
//  MullvadVPN
//
//  Created by pronebird on 17/11/2023.
//  Copyright © 2025 Mullvad VPN AB. All rights reserved.
//

import Combine
import MullvadSettings
import UIKit

/// The view controller providing the interface for editing the existing access method.
class EditAccessMethodViewController: UIViewController {
    typealias EditAccessMethodDataSource = UITableViewDiffableDataSource<
        EditAccessMethodSectionIdentifier,
        EditAccessMethodItemIdentifier
    >

    private let tableView = UITableView(frame: .zero, style: .insetGrouped)
    private let subject: CurrentValueSubject<AccessMethodViewModel, Never>
    private let interactor: EditAccessMethodInteractorProtocol
    private var alertPresenter: AlertPresenter
    private var cancellables = Set<AnyCancellable>()
    private var dataSource: EditAccessMethodDataSource?

    weak var delegate: EditAccessMethodViewControllerDelegate?

    init(
        subject: CurrentValueSubject<AccessMethodViewModel, Never>,
        interactor: EditAccessMethodInteractorProtocol,
        alertPresenter: AlertPresenter
    ) {
        self.subject = subject
        self.interactor = interactor
        self.alertPresenter = alertPresenter

        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .secondaryColor

        tableView.setAccessibilityIdentifier(.editAccessMethodView)
        tableView.backgroundColor = .secondaryColor
        tableView.delegate = self

        isModalInPresentation = true

        let title = createTitle()
        view.addConstrainedSubviews([title, tableView]) {
            title.pinEdgesToSuperviewMargins(PinnableEdges([.leading(7), .trailing(7), .top(0)]))
            tableView.pinEdgesToSuperview(.all().excluding(.top))
            tableView.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 4)
        }

        configureDataSource()
        configureNavigationItem()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        interactor.cancelProxyConfigurationTest()
    }

    private func createTitle() -> UIView {
        let label = UILabel()
        label.font = .mullvadBig
        label.adjustsFontForContentSizeCategory = true
        label.numberOfLines = 0
        label.lineBreakMode = .byWordWrapping
        label.lineBreakStrategy = []
        label.text = subject.value.navigationItemTitle
        label.textColor = UIColor.NavigationBar.titleColor
        return label
    }
}

// MARK: - UITableViewDelegate

extension EditAccessMethodViewController: UITableViewDelegate {
    func tableView(_ tableView: UITableView, shouldHighlightRowAt indexPath: IndexPath) -> Bool {
        guard let itemIdentifier = dataSource?.itemIdentifier(for: indexPath) else { return false }

        return itemIdentifier.isSelectable
    }

    func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        guard let itemIdentifier = dataSource?.itemIdentifier(for: indexPath) else { return }

        if case .methodSettings = itemIdentifier {
            delegate?.controllerShouldShowMethodSettings(self)
        }
    }

    func tableView(_ tableView: UITableView, heightForRowAt indexPath: IndexPath) -> CGFloat {
        UITableView.automaticDimension
    }

    func tableView(_ tableView: UITableView, viewForHeaderInSection section: Int) -> UIView? {
        guard let sectionIdentifier = dataSource?.snapshot().sectionIdentifiers[section] else { return nil }
        switch sectionIdentifier {
        case .enableMethod:
            var headerView: InfoHeaderView?

            if let headerConfig = subject.value.infoHeaderConfig {
                headerView = InfoHeaderView(config: headerConfig)

                headerView?.onAbout = { [weak self] in
                    guard let self, let infoModalConfig = subject.value.infoModalConfig else { return }
                    delegate?.controllerShouldShowMethodInfo(self, config: infoModalConfig)
                }
            }
            headerView?.directionalLayoutMargins = NSDirectionalEdgeInsets(
                top: 4,
                leading: 0,
                bottom: 16,
                trailing: 0
            )

            return headerView ?? UIView()
        default:
            return nil
        }
    }

    // Header height shenanigans to avoid extra spacing in testing sections when testing is NOT ongoing.
    func tableView(_ tableView: UITableView, heightForHeaderInSection section: Int) -> CGFloat {
        guard let sectionIdentifier = dataSource?.snapshot().sectionIdentifiers[section] else { return 0 }

        switch sectionIdentifier {
        case .methodSettings, .deleteMethod, .testMethod, .enableMethod:
            return UITableView.automaticDimension
        case .testingStatus:
            return subject.value.testingStatus == .initial ? 0 : UITableView.automaticDimension
        default:
            return 0
        }
    }

    func tableView(_ tableView: UITableView, viewForFooterInSection section: Int) -> UIView? {
        guard let sectionIdentifier = dataSource?.snapshot().sectionIdentifiers[section] else { return nil }
        if sectionIdentifier == .enableMethod && subject.value.canBeToggled {
            return nil
        }
        guard let sectionFooterText = sectionIdentifier.sectionFooter else { return nil }

        guard let headerView = tableView
            .dequeueReusableView(withIdentifier: AccessMethodHeaderFooterReuseIdentifier.primary)
        else { return nil }

        var contentConfiguration = UIListContentConfiguration.mullvadGroupedFooter(tableStyle: tableView.style)
        contentConfiguration.text = sectionFooterText

        headerView.contentConfiguration = contentConfiguration

        return headerView
    }

    // Footer height shenanigans to avoid extra spacing in testing sections when testing is NOT ongoing.
    func tableView(_ tableView: UITableView, heightForFooterInSection section: Int) -> CGFloat {
        guard let sectionIdentifier = dataSource?.snapshot().sectionIdentifiers[section] else { return 0 }
        let marginToDeleteMethodItem: CGFloat = 24

        switch sectionIdentifier {
        case .enableMethod, .methodSettings, .deleteMethod, .testMethod:
            return UITableView.automaticDimension
        case .testingStatus:
            switch subject.value.testingStatus {
            case .initial, .inProgress:
                return 0
            case .succeeded, .failed:
                return marginToDeleteMethodItem
            }
        case .cancelTest:
            return subject.value.testingStatus == .inProgress ? marginToDeleteMethodItem : 0
        }
    }

    // MARK: - Cell configuration

    private func dequeueCell(at indexPath: IndexPath, for itemIdentifier: EditAccessMethodItemIdentifier)
        -> UITableViewCell {
        let cell = tableView.dequeueReusableView(withIdentifier: itemIdentifier.cellIdentifier, for: indexPath)

        configureBackground(cell: cell, itemIdentifier: itemIdentifier)

        switch itemIdentifier {
        case .testMethod:
            configureTestMethod(cell, itemIdentifier: itemIdentifier)
        case .cancelTest:
            configureCancelTest(cell, itemIdentifier: itemIdentifier)
        case .testingStatus:
            configureTestingStatus(cell, itemIdentifier: itemIdentifier)
        case .deleteMethod:
            configureDeleteMethod(cell, itemIdentifier: itemIdentifier)
        case .enableMethod:
            configureEnableMethod(cell, itemIdentifier: itemIdentifier)
        case .methodSettings:
            configureMethodSettings(cell, itemIdentifier: itemIdentifier)
        }

        return cell
    }

    private func configureBackground(cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        guard let cell = cell as? DynamicBackgroundConfiguration else { return }

        guard !itemIdentifier.isClearBackground else {
            cell.setAutoAdaptingClearBackgroundConfiguration()
            return
        }

        cell.setAutoAdaptingBackgroundConfiguration(.mullvadListGroupedCell(), selectionType: .dimmed)
    }

    private func configureTestMethod(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = ButtonCellContentConfiguration()
        contentConfiguration.accessibilityIdentifier = .accessMethodTestButton
        contentConfiguration.text = itemIdentifier.text
        contentConfiguration.isEnabled = subject.value.testingStatus != .inProgress
        contentConfiguration.primaryAction = UIAction { [weak self] _ in
            self?.onTest()
        }
        cell.contentConfiguration = contentConfiguration
    }

    private func configureCancelTest(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = ButtonCellContentConfiguration()
        contentConfiguration.accessibilityIdentifier = .accessMethodTestButton
        contentConfiguration.text = itemIdentifier.text
        contentConfiguration.isEnabled = subject.value.testingStatus == .inProgress
        contentConfiguration.primaryAction = UIAction { [weak self] _ in
            self?.onCancelTest()
        }
        cell.contentConfiguration = contentConfiguration
    }

    private func configureTestingStatus(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = MethodTestingStatusCellContentConfiguration()
        contentConfiguration.status = subject.value.testingStatus.viewStatus
        cell.contentConfiguration = contentConfiguration
    }

    private func configureEnableMethod(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = SwitchCellContentConfiguration()
        contentConfiguration.accessibilityIdentifier = .accessMethodEnableSwitch
        contentConfiguration.text = itemIdentifier.text
        contentConfiguration.isOn = subject.value.isEnabled
        contentConfiguration.onChange = UIAction { [weak self] action in
            if let customSwitch = action.sender as? UISwitch {
                self?.subject.value.isEnabled = customSwitch.isOn
                self?.onSave()
            }
        }

        contentConfiguration.isEnabled = subject.value.canBeToggled
        cell.contentConfiguration = contentConfiguration
    }

    private func configureMethodSettings(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = UIListContentConfiguration.mullvadCell(tableStyle: tableView.style)
        contentConfiguration.text = itemIdentifier.text
        cell.contentConfiguration = contentConfiguration

        if let cell = cell as? CustomCellDisclosureHandling {
            cell.disclosureType = .chevron
        }
    }

    private func configureDeleteMethod(_ cell: UITableViewCell, itemIdentifier: EditAccessMethodItemIdentifier) {
        var contentConfiguration = ButtonCellContentConfiguration()
        contentConfiguration.style = .tableInsetGroupedDanger
        contentConfiguration.text = itemIdentifier.text
        contentConfiguration.accessibilityIdentifier = .deleteButton
        contentConfiguration.primaryAction = UIAction { [weak self] _ in
            self?.onDelete()
        }
        cell.contentConfiguration = contentConfiguration
    }

    // MARK: - Data source handling

    private func configureDataSource() {
        tableView.registerReusableViews(from: AccessMethodCellReuseIdentifier.self)
        tableView.registerReusableViews(from: AccessMethodHeaderFooterReuseIdentifier.self)

        dataSource = UITableViewDiffableDataSource(
            tableView: tableView,
            cellProvider: { [weak self] _, indexPath, itemIdentifier in
                self?.dequeueCell(at: indexPath, for: itemIdentifier)
            }
        )

        subject.withPreviousValue()
            .sink { [weak self] previousValue, newValue in
                self?.viewModelDidChange(previousValue: previousValue, newValue: newValue)
            }
            .store(in: &cancellables)
    }

    private func viewModelDidChange(previousValue: AccessMethodViewModel?, newValue: AccessMethodViewModel) {
        let animated = view.window != nil

        configureNavigationItem()
        updateDataSource(
            previousValue: previousValue,
            newValue: newValue,
            animated: animated
        )
    }

    private func updateDataSource(
        previousValue: AccessMethodViewModel?,
        newValue: AccessMethodViewModel,
        animated: Bool
    ) {
        var snapshot = NSDiffableDataSourceSnapshot<EditAccessMethodSectionIdentifier, EditAccessMethodItemIdentifier>()

        snapshot.appendSections([.enableMethod])
        snapshot.appendItems([.enableMethod], toSection: .enableMethod)

        // Add method settings if the access method is configurable.
        if newValue.method.hasProxyConfiguration {
            snapshot.appendSections([.methodSettings])
            snapshot.appendItems([.methodSettings], toSection: .methodSettings)
        }

        snapshot.appendSections([.testMethod])
        snapshot.appendItems([.testMethod], toSection: .testMethod)

        // Reconfigure the test button on status changes.
        if let previousValue, previousValue.testingStatus != newValue.testingStatus {
            snapshot.reconfigureItems([.testMethod])
        }

        snapshot.appendSections([.testingStatus])
        snapshot.appendSections([.cancelTest])

        // Add test status below the test button.
        if newValue.testingStatus != .initial {
            snapshot.appendItems([.testingStatus], toSection: .testingStatus)

            if let previousValue, previousValue.testingStatus != newValue.testingStatus {
                snapshot.reconfigureItems([.testingStatus])
            }

            // Show cancel test button below test status.
            if newValue.testingStatus == .inProgress {
                snapshot.appendItems([.cancelTest], toSection: .cancelTest)
            }
        }

        // Add delete button for user-defined access methods.
        if !newValue.method.isPermanent {
            snapshot.appendSections([.deleteMethod])
            snapshot.appendItems([.deleteMethod], toSection: .deleteMethod)
        }

        dataSource?.apply(snapshot, animatingDifferences: animated)
    }

    // MARK: - Misc

    private func configureNavigationItem() {
        navigationItem.largeTitleDisplayMode = .never
        navigationItem.title = ""
    }

    private func onSave() {
        interactor.saveAccessMethod()
    }

    private func onDelete() {
        let methodName = subject.value.name.isEmpty
            ? NSLocalizedString(
                "METHOD_SETTINGS_SAVE_PROMPT",
                tableName: "APIAccess",
                value: "method?",
                comment: ""
            )
            : subject.value.name

        let presentation = AlertPresentation(
            id: "api-access-methods-delete-method-alert",
            icon: .alert,
            message: NSLocalizedString(
                "METHOD_SETTINGS_DELETE_PROMPT",
                tableName: "APIAccess",
                value: "Delete \(methodName)?",
                comment: ""
            ),
            buttons: [
                AlertAction(
                    title: NSLocalizedString(
                        "METHOD_SETTINGS_DELETE_BUTTON",
                        tableName: "APIAccess",
                        value: "Delete",
                        comment: ""
                    ),
                    style: .destructive,
                    accessibilityId: .accessMethodConfirmDeleteButton,
                    handler: { [weak self] in
                        guard let self else { return }
                        interactor.deleteAccessMethod()
                        delegate?.controllerDidDeleteAccessMethod(self)
                    }
                ),
                AlertAction(
                    title: NSLocalizedString(
                        "METHOD_SETTINGS_CANCEL_BUTTON",
                        tableName: "APIAccess",
                        value: "Cancel",
                        comment: ""
                    ),
                    style: .default
                ),
            ]
        )

        alertPresenter.showAlert(presentation: presentation, animated: true)
    }

    private func onTest() {
        interactor.startProxyConfigurationTest()
    }

    private func onCancelTest() {
        interactor.cancelProxyConfigurationTest()
    }
} // swiftlint:disable:this file_length
