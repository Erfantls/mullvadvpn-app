//
//  SettingsDNSInfoCell.swift
//  MullvadVPN
//
//  Created by Jon Petersson on 2023-07-07.
//  Copyright © 2025 Mullvad VPN AB. All rights reserved.
//

import UIKit

class SettingsDNSInfoCell: UITableViewCell {
    let titleLabel = UILabel()

    override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
        super.init(style: style, reuseIdentifier: reuseIdentifier)

        backgroundColor = .secondaryColor
        contentView.directionalLayoutMargins = UIMetrics.SettingsCell.layoutMargins

        titleLabel.adjustsFontForContentSizeCategory = true
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.textColor = UIColor.Cell.titleTextColor
        titleLabel.numberOfLines = 0
        titleLabel.setContentCompressionResistancePriority(.defaultHigh, for: .vertical)
        titleLabel.setContentHuggingPriority(.defaultLow, for: .vertical)

        contentView.addConstrainedSubviews([titleLabel]) {
            titleLabel.pinEdgesToSuperviewMargins()
        }
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
