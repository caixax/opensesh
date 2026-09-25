// Settings > About: app name, version, license, system details, the logs and settings folders,
// and a summary of the third-party notices (the full texts ship in THIRD_PARTY_NOTICES.md).
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property var credits: [
        {
            name: qsTr("Lucide icons"),
            license: qsTr("ISC License (icons derived from Feather: MIT License)")
        },
        {
            name: qsTr("Tabler Icons"),
            license: qsTr("MIT License")
        },
        {
            name: qsTr("Simple Icons"),
            license: qsTr("CC0 1.0 (the logos are trademarks of their owners)")
        },
        {
            name: qsTr("Inter"),
            license: qsTr("SIL Open Font License 1.1")
        },
        {
            name: qsTr("JetBrains Mono"),
            license: qsTr("SIL Open Font License 1.1")
        },
        {
            name: qsTr("Qt"),
            license: qsTr("GNU Lesser General Public License v3")
        }
    ]

    // Display name of a Rust `std::env::consts::OS` value.
    function osName(os: string): string {
        switch (os) {
        case "windows":
            return qsTr("Windows");
        case "linux":
            return qsTr("Linux");
        case "macos":
            return qsTr("macOS");
        case "freebsd":
            return qsTr("FreeBSD");
        default:
            return os;
        }
    }

    title: qsTr("About")
    description: qsTr("Version, license and credits.")

    // App identity.
    OsCard {
        width: parent.width
        padding: Theme.spacingLg

        Row {
            width: parent.width
            spacing: Theme.spacingLg

            // The placeholder app logo (PLAN §7): the door-open glyph on an accent squircle.
            Rectangle {
                id: logo

                anchors.verticalCenter: parent.verticalCenter
                width: Theme.spacingXxl * 2
                height: width
                radius: Theme.radiusCard * 1.5
                color: Theme.accent
                Accessible.ignored: true

                OsIcon {
                    anchors.centerIn: parent
                    name: "door-open"
                    size: Theme.spacingXxl
                    color: Theme.accentText
                }
            }

            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - logo.width - parent.spacing
                spacing: Theme.spacingXs

                OsText {
                    width: parent.width
                    text: qsTr("OpenSesh")
                    size: "title"
                    horizontalAlignment: Text.AlignLeft
                    Accessible.role: Accessible.Heading
                }

                OsText {
                    width: parent.width
                    text: qsTr("Version %1").arg(AppInfo.version)
                    horizontalAlignment: Text.AlignLeft
                }

                OsText {
                    width: parent.width
                    text: qsTr("A remote connections client for SSH, SFTP, tunnels, terminals, RDP and VNC.")
                    muted: true
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
                    horizontalAlignment: Text.AlignLeft
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Details")

        SettingsRow {
            label: qsTr("License")

            OsText {
                width: parent.width
                height: Math.max(Theme.controlHeightSmall, implicitHeight)
                text: qsTr("GNU General Public License v3.0 or later (GPL-3.0-or-later)")
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }

        SettingsRow {
            label: qsTr("App ID")

            OsText {
                width: parent.width
                height: Math.max(Theme.controlHeightSmall, implicitHeight)
                text: AppInfo.appId
                font.family: Theme.monoFontFamily
                horizontalAlignment: Text.AlignLeft
            }
        }

        SettingsRow {
            label: qsTr("System")

            OsText {
                width: parent.width
                height: Math.max(Theme.controlHeightSmall, implicitHeight)
                text: Platform.desktopName.length > 0
                      ? qsTr("%1, %2 desktop").arg(page.osName(Platform.os)).arg(Platform.desktopName)
                      : page.osName(Platform.os)
                horizontalAlignment: Text.AlignLeft
            }
        }

        SettingsRow {
            label: qsTr("Build")

            OsText {
                width: parent.width
                height: Math.max(Theme.controlHeightSmall, implicitHeight)
                text: Platform.debugBuild ? qsTr("Debug") : qsTr("Release")
                horizontalAlignment: Text.AlignLeft
            }
        }

        SettingsRow {
            label: qsTr("Folders")
            helpText: qsTr("The logs help with bug reports. The settings folder holds config.toml.")

            Flow {
                width: parent.width
                spacing: Theme.spacingSm

                OsButton {
                    text: qsTr("Open logs folder")
                    iconName: "scroll-text"
                    onClicked: Qt.openUrlExternally(AppInfo.logsFolder)
                }

                OsButton {
                    text: qsTr("Open settings folder")
                    iconName: "folder-open"
                    onClicked: Qt.openUrlExternally(AppInfo.configFolder)
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Third-party notices")
        description: qsTr("OpenSesh bundles these works, each under its own license. The full texts are in THIRD_PARTY_NOTICES.md and the LICENSES folders that ship with OpenSesh.")

        Repeater {
            model: page.credits

            delegate: SettingsRow {
                id: creditRow

                required property var modelData

                label: modelData.name

                OsText {
                    width: parent.width
                    height: Math.max(Theme.controlHeightSmall, implicitHeight)
                    text: creditRow.modelData.license
                    muted: true
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
                    horizontalAlignment: Text.AlignLeft
                }
            }
        }
    }
}
