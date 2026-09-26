// Settings > Security (PLAN §8, Sprint 6): who holds the key of the vault (the system keyring or
// a master password), remembering it on this computer, locking after a while without use, and
// resetting a vault whose master password is lost. Every change goes through the Keychain
// worker; the dialogs live in the shell.
import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property Item shell: WindowRegistry.mainShell
    readonly property bool hasVault: Keychain.vaultStatus === "locked" || Keychain.vaultStatus === "unlocked"
    readonly property bool passwordVault: hasVault && Keychain.protection === "password"
    readonly property var idleChoices: [
        { value: 0, text: qsTr("Never") },
        { value: 5, text: qsTr("After 5 minutes") },
        { value: 15, text: qsTr("After 15 minutes") },
        { value: 30, text: qsTr("After 30 minutes") },
        { value: 60, text: qsTr("After 1 hour") },
        { value: 240, text: qsTr("After 4 hours") }
    ]

    // Runs `action` once the vault is open (asking for the master password if needed).
    function whenUnlocked(action) {
        if (Keychain.vaultStatus === "locked")
            shell.unlockVault(action);
        else
            action();
    }

    title: qsTr("Security")
    description: qsTr("The vault keeps your passwords and private keys encrypted. Its key is held by your system keyring or by a master password.")

    SettingsNotice {
        width: parent.width
        visible: Keychain.vaultStatus === "unreadable"
        kind: "danger"
        title: qsTr("The vault can't be read")
        lines: [Keychain.vaultProblem, qsTr("It is left as it is. Reset it to start a new one.")]
    }

    SettingsNotice {
        width: parent.width
        visible: !Keychain.keyringAvailable
        kind: "info"
        title: qsTr("No system keyring")
        lines: [qsTr("Secrets can only be kept with a master password on this system."), Keychain.keyringProblem]
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Vault")

        SettingsRow {
            label: qsTr("Status")

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: {
                    if (!page.hasVault)
                        return Keychain.vaultStatus === "unreadable" ? qsTr("Damaged or from a newer OpenSesh")
                                                                     : qsTr("No vault yet: one is created when you first save a secret.");
                    if (Keychain.protection === "keyring")
                        return qsTr("Unlocked by your system keyring");
                    if (Keychain.remembered)
                        return qsTr("Protected by a master password, remembered on this computer");
                    return Keychain.vaultStatus === "unlocked" ? qsTr("Protected by a master password, unlocked")
                                                               : qsTr("Protected by a master password, locked");
                }
            }
        }

        SettingsRow {
            label: qsTr("Master password")
            helpText: page.passwordVault ? qsTr("Removing it gives the key back to the system keyring.")
                                         : qsTr("With a master password, the vault can't be opened without it, even from your account.")

            Row {
                spacing: Theme.spacingSm

                OsButton {
                    visible: !page.passwordVault && Keychain.vaultStatus !== "unreadable"
                    text: qsTr("Set a master password…")
                    iconName: "lock"
                    onClicked: {
                        if (!page.hasVault)
                            page.shell.showMasterPassword("create");
                        else
                            page.whenUnlocked(() => page.shell.showMasterPassword("set"));
                    }
                }

                OsButton {
                    visible: page.passwordVault
                    text: qsTr("Change…")
                    onClicked: page.whenUnlocked(() => page.shell.showMasterPassword("change"))
                }

                OsButton {
                    visible: page.passwordVault && Keychain.keyringAvailable
                    variant: "ghost"
                    text: qsTr("Remove…")
                    onClicked: page.whenUnlocked(() => page.shell.showMasterPassword("remove"))
                }
            }
        }

        SettingsRow {
            visible: page.passwordVault
            label: qsTr("Remember on this computer")
            helpText: qsTr("The system keyring keeps the vault key, so the vault opens without the master password here.")

            OsSwitch {
                checked: Keychain.remembered
                enabled: Keychain.keyringAvailable && Keychain.vaultStatus === "unlocked" && !Keychain.busy
                Accessible.name: qsTr("Remember on this computer")
                onToggled: {
                    const wanted = checked;
                    KeychainTasks.run(Keychain.setRemember(wanted), (code, detail) => {
                        if (code.length > 0)
                            Toasts.show(KeychainTasks.message(code, detail), "danger");
                    });
                }
            }
        }

        SettingsRow {
            visible: page.passwordVault && !Keychain.remembered
            label: qsTr("Lock automatically")
            helpText: qsTr("When OpenSesh isn't used for this long, the vault locks and asks for the master password again.")

            OsComboBox {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                model: page.idleChoices
                textRole: "text"
                valueRole: "value"
                currentIndex: Math.max(0, page.idleChoices.findIndex(choice => choice.value === AppSettings.lockAfterMinutes))
                displayText: currentIndex >= 0 && page.idleChoices[currentIndex].value === AppSettings.lockAfterMinutes
                             ? currentText : qsTr("After %n minute(s)", "", AppSettings.lockAfterMinutes)
                Accessible.name: qsTr("Lock automatically")
                onActivated: AppSettings.lockAfterMinutes = currentValue
            }
        }

        SettingsRow {
            visible: page.passwordVault && !Keychain.remembered
            label: qsTr("Lock now")

            OsButton {
                text: Keychain.vaultStatus === "unlocked" ? qsTr("Lock the vault") : qsTr("Unlock…")
                iconName: Keychain.vaultStatus === "unlocked" ? "lock" : "lock-open"
                onClicked: {
                    if (Keychain.vaultStatus === "unlocked")
                        Keychain.lock();
                    else
                        page.shell.unlockVault(null);
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        visible: page.hasVault || Keychain.vaultStatus === "unreadable"
        title: qsTr("Reset")
        description: qsTr("For a forgotten master password or a damaged vault: deletes every saved password and private key.")

        SettingsRow {
            label: qsTr("Reset the vault")

            OsButton {
                variant: "danger"
                text: qsTr("Reset…")
                iconName: "trash-2"
                onClicked: page.shell.showVaultReset()
            }
        }
    }

    SettingsGroup {
        width: parent.width
        visible: Keychain.filePath.length > 0
        title: qsTr("Files")

        SettingsRow {
            label: qsTr("Keychain")
            helpText: qsTr("Identities and public keys, readable; secrets are only references into the vault.")

            OsText {
                width: parent.width
                text: Keychain.filePath
                elide: Text.ElideMiddle
            }
        }
    }
}
