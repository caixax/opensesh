// Settings > General (PLAN §6.1): language, startup and closing behavior, update checks and the
// settings file. Every control reads and writes AppSettings live.
// Functions (for smoke tests and screenshots): openRestoreDialog(), closeRestoreDialog().
import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    // Language codes offered by Platform, with their labels. Languages are named in their own
    // language (Platform.languageName), so only "system" is translated.
    readonly property var languageOptions: Platform.languages().map(code => ({
                value: code,
                text: code === "system" ? qsTr("System default") : Platform.languageName(code)
            }))
    readonly property bool hasFileNotice: AppSettings.readOnly || AppSettings.warnings.length > 0

    function openRestoreDialog() {
        restoreDialog.open();
    }

    function closeRestoreDialog() {
        restoreDialog.close();
    }

    function copyConfigPath() {
        pathField.selectAll();
        pathField.copy();
        pathField.deselect();
        Toasts.show(qsTr("Settings file path copied."), "success");
    }

    title: qsTr("General")
    description: qsTr("Language, startup and closing behavior, updates and the settings file.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Language")

        SettingsRow {
            label: qsTr("Interface language")
            helpText: qsTr("Applies immediately. The system default follows your operating system's language.")

            OsComboBox {
                id: languageBox

                width: Math.min(parent.width, Theme.spacingXxl * 9)
                textRole: "text"
                valueRole: "value"
                model: page.languageOptions
                // `model` is read so the index is set again after the model is rebuilt.
                currentIndex: model && count > 0 ? indexOfValue(AppSettings.language) : -1
                // A configured language without a bundled translation still shows its code.
                displayText: currentIndex >= 0 ? currentText : AppSettings.language

                Accessible.name: qsTr("Interface language")

                // Main.qml applies the language whenever the setting changes.
                onActivated: AppSettings.language = currentValue
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Startup and closing")

        SettingsRow {
            label: qsTr("When the last tab closes")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("onLastTabClosed")
                labels: ({
                        keep_window: qsTr("Keep the window open"),
                        quit: qsTr("Quit OpenSesh")
                    })
                value: AppSettings.onLastTabClosed
                Accessible.name: qsTr("When the last tab closes")
                onPicked: value => AppSettings.onLastTabClosed = value
            }
        }

        SettingsRow {
            label: qsTr("Restore sessions at startup")
            helpText: qsTr("Reopens the tabs that were open when you quit. Takes effect from Sprint 4, when tabs and workspaces arrive.")

            OsSwitch {
                checked: AppSettings.restoreSessions
                Accessible.name: qsTr("Restore sessions at startup")
                onToggled: AppSettings.restoreSessions = checked
            }
        }

        SettingsRow {
            label: qsTr("Confirm before closing with active sessions")
            helpText: qsTr("Asks before closing a tab or the window while sessions are connected.")

            OsSwitch {
                checked: AppSettings.confirmCloseWithSessions
                Accessible.name: qsTr("Confirm before closing with active sessions")
                onToggled: AppSettings.confirmCloseWithSessions = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Updates")

        SettingsRow {
            label: qsTr("Check for updates")
            helpText: qsTr("Off by default. When enabled, OpenSesh contacts GitHub Releases; nothing else is ever sent. The check itself arrives in Sprint 18.")

            OsSwitch {
                checked: AppSettings.checkForUpdates
                Accessible.name: qsTr("Check for updates")
                onToggled: AppSettings.checkForUpdates = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Settings file")
        description: qsTr("Changes are saved to config.toml as you make them, and edits to the file are applied while OpenSesh runs.")

        SettingsNotice {
            width: parent.width
            visible: page.hasFileNotice
            kind: "warning"
            title: AppSettings.readOnlyReason === "newer" ? qsTr("This file was written by a newer version of OpenSesh")
                 : AppSettings.readOnlyReason === "unreadable" ? qsTr("This file could not be read")
                 : qsTr("Some settings in the file were not valid")
            lines: {
                const result = [];
                if (AppSettings.readOnlyReason === "newer")
                    result.push(qsTr("Your changes apply now but are not saved, so the newer file is never overwritten."));
                else if (AppSettings.readOnlyReason === "unreadable")
                    result.push(qsTr("Your changes apply now but are not saved until the file is fixed, so your edits are never overwritten. Restore defaults replaces it and keeps a backup."));
                for (const warning of AppSettings.warnings)
                    result.push(warning);
                return result;
            }
        }

        SettingsRow {
            label: qsTr("Location")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsTextField {
                    id: pathField

                    width: parent.width
                    readOnly: true
                    text: AppSettings.configPath.length > 0 ? AppSettings.configPath : qsTr("Not available")
                    font.family: Theme.monoFontFamily
                    Accessible.name: qsTr("Settings file location")
                }

                Flow {
                    width: parent.width
                    spacing: Theme.spacingSm

                    OsButton {
                        text: qsTr("Copy path")
                        iconName: "copy"
                        enabled: AppSettings.configPath.length > 0
                        onClicked: page.copyConfigPath()
                    }

                    OsButton {
                        text: qsTr("Open folder")
                        iconName: "folder-open"
                        onClicked: Qt.openUrlExternally(AppInfo.configFolder)
                    }
                }
            }
        }

        SettingsRow {
            label: qsTr("Restore defaults")
            helpText: qsTr("Puts every General and Appearance setting back to its default value.")

            OsButton {
                text: qsTr("Restore defaults…")
                iconName: "rotate-ccw"
                variant: "danger"
                onClicked: page.openRestoreDialog()
            }
        }
    }

    OsDialog {
        id: restoreDialog

        title: qsTr("Restore default settings?")
        acceptText: qsTr("Restore defaults")
        dangerous: true

        onAccepted: {
            AppSettings.resetToDefaults();
            Toasts.show(qsTr("Settings restored to their defaults."), "success");
        }

        Column {
            width: Math.min(Theme.spacingXxl * 12,
                            restoreDialog.maxWidth - restoreDialog.leftPadding - restoreDialog.rightPadding)
            spacing: Theme.spacingSm

            OsText {
                width: parent.width
                text: qsTr("Every General and Appearance setting goes back to its default value, including the language, theme and layout.")
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }

            OsText {
                width: parent.width
                text: qsTr("This can't be undone.")
                muted: true
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }
}
