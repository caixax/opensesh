pragma ComponentBehavior: Bound

// Keychain view (PLAN §5.4, Sprint 6). Left, the vault (who holds its key, locked or not, with
// Lock/Unlock) and the sections with counts: Identities, Keys, Agents, Known hosts. Right, a
// search field, the section's actions and its list:
// - Identities: name, user, key and whether a password is saved; edit, delete.
// - Keys: name, type, fingerprint, who uses it; copy the public key, export the public or the
//   private key, rename, delete.
// - Agents: the keys of the running SSH agents (SSH_AUTH_SOCK, the Windows OpenSSH agent,
//   Pageant), read-only.
// - Known hosts: ~/.ssh/known_hosts and OpenSesh's own file, read-only until the SSH client
//   (Sprint 7) checks and adds host keys.
// Every change goes through the Keychain singleton; the dialogs live in the shell.
// Functions: showSection(id), smokeSteps(smoke).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    property string section: "identities"
    // The row a menu or dialog acts on.
    property var target: null
    readonly property var identities: JSON.parse(Keychain.identities || "[]")
    readonly property var keys: JSON.parse(Keychain.keys || "[]")
    readonly property var agents: JSON.parse(Keychain.agents || "[]")
    readonly property var knownFiles: JSON.parse(Keychain.knownHosts || "[]")
    readonly property string query: search.text.trim().toLowerCase()
    readonly property int agentKeyCount: agents.reduce((sum, agent) => sum + agent.keys.length, 0)
    readonly property int knownHostCount: knownFiles.reduce((sum, file) => sum + file.entries.length, 0)
    readonly property var sections: [
        { id: "identities", text: qsTr("Identities"), iconName: "user", count: identities.length },
        { id: "keys", text: qsTr("Keys"), iconName: "key-round", count: keys.length },
        { id: "agents", text: qsTr("Agents"), iconName: "plug-zap", count: agentKeyCount },
        { id: "known", text: qsTr("Known hosts"), iconName: "server", count: knownHostCount }
    ]
    // The rows of the current section that match the search: {kind, ...}.
    readonly property var rows: {
        const q = query;
        const hit = parts => q.length === 0 || parts.some(part => String(part ?? "").toLowerCase().indexOf(q) >= 0);
        switch (section) {
        case "identities":
            return identities.filter(item => hit([item.name, item.user, item.keyName, item.notes]))
                             .map(item => Object.assign({ kind: "identity" }, item));
        case "keys":
            return keys.filter(item => hit([item.name, item.label, item.fingerprint, item.comment]))
                       .map(item => Object.assign({ kind: "key" }, item));
        case "agents": {
            const out = [];
            for (const agent of agents) {
                out.push({ kind: "header", title: agentName(agent.kind), subtitle: agent.location, state: agent.error });
                for (const key of agent.keys.filter(key => hit([key.comment, key.label, key.fingerprint])))
                    out.push(Object.assign({ kind: "agentKey" }, key));
            }
            return out;
        }
        default: {
            const out = [];
            for (const file of knownFiles) {
                out.push({ kind: "header", title: file.path, subtitle: file.problem, state: "" });
                for (const entry of file.entries.filter(entry => hit([entry.hosts, entry.keyType, entry.fingerprint, entry.comment])))
                    out.push(Object.assign({ kind: "knownHost" }, entry));
            }
            return out;
        }
        }
    }
    readonly property bool sectionEmpty: section === "identities" ? identities.length === 0
                                       : section === "keys" ? keys.length === 0
                                       : section === "agents" ? agentKeyCount === 0
                                       : knownHostCount === 0

    function showSection(id) {
        section = id;
        search.text = "";
        if (id === "agents")
            Keychain.refreshAgents();
        else if (id === "known")
            Keychain.refreshKnownHosts();
    }

    function agentName(kind) {
        switch (kind) {
        case "openssh":
            return qsTr("OpenSSH agent (Windows)");
        case "pageant":
            return qsTr("Pageant");
        default:
            return qsTr("SSH agent (SSH_AUTH_SOCK)");
        }
    }

    function agentState(state) {
        switch (state) {
        case "not-running":
            return qsTr("Not running");
        case "timeout":
            return qsTr("Did not answer");
        case "refused":
            return qsTr("Refused to list its keys");
        case "failed":
            return qsTr("Could not be reached");
        default:
            return "";
        }
    }

    function copyPublicKey(key) {
        Platform.copyText(key.publicKey);
        Toasts.show(qsTr("Public key of “%1” copied.").arg(key.name), "success");
    }

    function openMenu(row, item) {
        target = row;
        if (row.kind === "identity")
            identityMenu.popup(item, 0, item.height);
        else if (row.kind === "key")
            keyMenu.popup(item, 0, item.height);
    }

    function askDelete(row) {
        target = row;
        deleteDialog.open();
    }

    function deleteTarget() {
        const row = target;
        if (!row)
            return;
        const token = row.kind === "identity" ? Keychain.deleteIdentity(row.id) : Keychain.deleteKey(row.id);
        KeychainTasks.run(token, (code, detail) => {
            if (code.length > 0)
                Toasts.show(KeychainTasks.message(code, detail), "danger");
        });
    }

    function smokeSteps(smoke) {
        let token = 0;
        let result = null;
        const wait = check => {
            const step = () => (result === null ? [step] : check());
            return step;
        };
        // Polls until the wait after wrong passwords is over.
        const waitOver = () => (Keychain.waitUntil > Date.now() ? [waitOver] : null);
        const track = newToken => {
            result = null;
            token = KeychainTasks.run(newToken, (code, detail, value) => result = { code: code, detail: detail, value: value });
        };
        return [
            () => showSection("identities"),
            () => track(Keychain.saveIdentity(JSON.stringify({ id: "", name: "smoke", user: "tester", key: "", notes: "" }),
                                              "set", "smoke-password")),
            wait(() => {
                if (result.code !== "")
                    smoke.fail("saving an identity with a password failed: " + result.code);
                else if (Keychain.vaultStatus !== "unlocked" || Keychain.protection !== "keyring")
                    smoke.fail("the first secret did not create a keyring vault");
            }),
            () => showSection("keys"),
            () => track(Keychain.generateKey("ed25519", "smoke key", "smoke@test")),
            wait(() => {
                if (result.code !== "" || view.keys.length !== 1)
                    smoke.fail("generating a key failed: " + result.code);
            }),
            () => openMenu(rows[0], list.itemAtIndex(0) ?? list),
            () => keyMenu.close(),
            () => shell.editIdentity(view.identities[0].id),
            () => shell.closeKeychainDialogs(),
            () => shell.generateKey(),
            () => shell.closeKeychainDialogs(),
            () => shell.importKey(),
            () => shell.closeKeychainDialogs(),
            // A master password, a lock, and the wait after wrong passwords.
            () => track(Keychain.setMasterPassword("smoke master password", false)),
            wait(() => {
                if (result.code !== "" || Keychain.protection !== "password")
                    smoke.fail("setting a master password failed: " + result.code);
            }),
            () => track(Keychain.lock()),
            wait(() => {
                if (Keychain.vaultStatus !== "locked")
                    smoke.fail("the vault did not lock");
                shell.unlockVault(null);
            }),
            () => shell.closeKeychainDialogs(),
            () => track(Keychain.unlock("wrong one")),
            wait(() => track(Keychain.unlock("wrong two"))),
            wait(() => track(Keychain.unlock("wrong three"))),
            wait(() => {
                if (result.code !== "wrong-password" || KeychainTasks.waitSeconds() <= 0)
                    smoke.fail("three wrong passwords did not start a wait: " + result.code);
                track(Keychain.unlock("smoke master password"));
            }),
            wait(() => {
                if (result.code !== "wait" || Keychain.vaultStatus !== "locked")
                    smoke.fail("the right password was tried during the wait: " + result.code);
                shell.unlockVault(null);
            }),
            () => shell.closeKeychainDialogs(),
            waitOver,
            () => track(Keychain.unlock("smoke master password")),
            wait(() => {
                if (result.code !== "" || Keychain.vaultStatus !== "unlocked")
                    smoke.fail("the master password did not unlock the vault after the wait: " + result.code);
                else
                    console.info("smoke test: the keychain kept an identity and a key, locked, waited after wrong passwords, then unlocked");
            }),
            () => showSection("agents"),
            () => showSection("known"),
            () => showSection("identities")
        ];
    }

    Connections {
        target: view.shell

        function onActiveViewChanged() {
            if (view.shell.activeView === "keychain" && (view.section === "agents" || view.section === "known"))
                view.showSection(view.section);
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Vault and sections.
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: Math.round(Math.max(Theme.spacingXxl * 5, Math.min(Theme.spacingXxl * 7, view.width * 0.26)))
            color: Theme.surface
            border.width: 0

            Rectangle {
                anchors.right: parent.right
                width: Theme.borderWidth
                height: parent.height
                color: Theme.border
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Theme.spacingMd
                spacing: Theme.spacingMd

                OsCard {
                    id: vaultCard

                    Layout.fillWidth: true

                    readonly property string vaultState: Keychain.vaultStatus
                    readonly property bool passwordLocked: vaultState === "locked" && Keychain.protection === "password"
                    // Held by the keyring, but the keyring doesn't have its key (a data folder copied
                    // from another computer, or a keyring that is locked or missing).
                    readonly property bool keyMissing: vaultState === "locked" && Keychain.protection === "keyring"

                    Column {
                        width: parent.width
                        spacing: Theme.spacingSm

                        Row {
                            width: parent.width
                            spacing: Theme.spacingSm

                            OsIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: vaultCard.vaultState === "unreadable" ? "triangle-alert"
                                    : vaultCard.vaultState === "locked" ? "lock"
                                    : Keychain.protection === "password" ? "lock-open" : "shield-check"
                                size: Theme.iconSize
                                color: vaultCard.vaultState === "unreadable" ? Theme.danger
                                     : vaultCard.vaultState === "locked" ? Theme.warning : Theme.success
                            }

                            OsText {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - Theme.iconSize - Theme.spacingSm
                                font.weight: Font.DemiBold
                                text: vaultCard.vaultState === "missing" ? qsTr("No vault yet")
                                    : vaultCard.vaultState === "unreadable" ? qsTr("Vault can't be read")
                                    : vaultCard.vaultState === "locked" ? qsTr("Vault locked") : qsTr("Vault unlocked")
                            }
                        }

                        OsText {
                            width: parent.width
                            muted: true
                            size: "small"
                            wrapMode: Text.Wrap
                            elide: Text.ElideNone
                            text: vaultCard.vaultState === "missing" ? qsTr("Created with your first password or key.")
                                : vaultCard.vaultState === "unreadable" ? Keychain.vaultProblem
                                : vaultCard.keyMissing ? qsTr("The system keyring doesn't have its key: the data may come from another computer, or the keyring is locked.")
                                : Keychain.protection === "keyring" ? qsTr("Held by your system keyring.")
                                : Keychain.remembered ? qsTr("Master password, remembered on this computer.")
                                : qsTr("Protected by your master password.")
                        }

                        OsButton {
                            visible: vaultCard.passwordLocked || (vaultCard.vaultState === "unlocked" && Keychain.protection === "password" && !Keychain.remembered)
                            text: vaultCard.passwordLocked ? qsTr("Unlock…") : qsTr("Lock")
                            iconName: vaultCard.passwordLocked ? "lock-open" : "lock"
                            onClicked: {
                                if (vaultCard.passwordLocked)
                                    view.shell.unlockVault(null);
                                else
                                    Keychain.lock();
                            }
                        }

                        OsButton {
                            visible: vaultCard.keyMissing
                            text: qsTr("Try again")
                            iconName: "refresh-cw"
                            onClicked: view.shell.unlockVault(null)
                        }

                        OsButton {
                            visible: vaultCard.vaultState === "unreadable" || vaultCard.keyMissing
                            variant: "danger"
                            text: qsTr("Reset…")
                            onClicked: view.shell.showVaultReset()
                        }

                        OsButton {
                            visible: vaultCard.vaultState !== "unreadable"
                            variant: "ghost"
                            text: qsTr("Security settings")
                            iconName: "settings"
                            onClicked: view.shell.openSettings("security")
                        }
                    }
                }

                ListView {
                    id: sectionList

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    model: view.sections
                    spacing: Theme.spacingXs
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Keychain sections")

                    delegate: OsListRow {
                        id: sectionRow

                        required property var modelData

                        width: ListView.view.width
                        text: modelData.text
                        iconName: modelData.iconName
                        selected: view.section === modelData.id
                        onClicked: view.showSection(modelData.id)

                        OsText {
                            visible: sectionRow.modelData.count > 0
                            text: sectionRow.modelData.count
                            muted: true
                            size: "small"
                        }
                    }
                }
            }
        }

        // The section's list.
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: Theme.spacingLg
            spacing: Theme.spacingMd

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingSm

                OsSearchField {
                    id: search

                    Layout.fillWidth: true
                    Layout.maximumWidth: Theme.spacingXxl * 12
                    placeholderText: view.section === "identities" ? qsTr("Search identities")
                                   : view.section === "keys" ? qsTr("Search keys")
                                   : view.section === "agents" ? qsTr("Search agent keys")
                                   : qsTr("Search known hosts")
                }

                Item {
                    Layout.fillWidth: true
                }

                OsButton {
                    visible: view.section === "identities"
                    variant: "primary"
                    text: qsTr("New identity")
                    iconName: "plus"
                    enabled: !Keychain.readOnly
                    onClicked: view.shell.newIdentity()
                }

                OsButton {
                    visible: view.section === "keys"
                    text: qsTr("Import…")
                    iconName: "import"
                    enabled: !Keychain.readOnly
                    onClicked: view.shell.importKey()
                }

                OsButton {
                    visible: view.section === "keys"
                    variant: "primary"
                    text: qsTr("Generate key")
                    iconName: "plus"
                    enabled: !Keychain.readOnly
                    onClicked: view.shell.generateKey()
                }

                OsButton {
                    visible: view.section === "agents" || view.section === "known"
                    text: qsTr("Refresh")
                    iconName: "refresh-cw"
                    onClicked: view.showSection(view.section)
                }
            }

            SettingsNotice {
                Layout.fillWidth: true
                visible: Keychain.readOnly || JSON.parse(Keychain.problems || "[]").length > 0
                kind: Keychain.readOnly ? "danger" : "warning"
                title: Keychain.readOnly ? qsTr("keychain.toml is read-only") : qsTr("Some entries of keychain.toml were fixed or skipped")
                lines: JSON.parse(Keychain.problems || "[]").slice(0, 4)
            }

            SettingsNotice {
                Layout.fillWidth: true
                visible: view.section === "known"
                kind: "info"
                title: qsTr("Read-only for now")
                lines: [qsTr("Checking host keys, and adding or removing them, arrive with the built-in SSH client (Sprint 7).")]
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: list

                    anchors.fill: parent
                    visible: !view.sectionEmpty
                    model: view.rows
                    clip: true
                    spacing: Theme.spacingXs
                    boundsBehavior: Flickable.StopAtBounds
                    reuseItems: true
                    Accessible.role: Accessible.List
                    Accessible.name: view.sections.find(entry => entry.id === view.section)?.text ?? ""

                    delegate: Item {
                        id: rowItem

                        required property var modelData
                        required property int index
                        readonly property bool header: modelData.kind === "header"

                        width: ListView.view.width
                        height: header ? headerColumn.implicitHeight + Theme.spacingMd : row.implicitHeight

                        Column {
                            id: headerColumn

                            visible: rowItem.header
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: Theme.spacingXs
                            width: parent.width
                            spacing: Theme.spacingXs

                            OsText {
                                width: parent.width
                                font.weight: Font.DemiBold
                                text: rowItem.modelData.title ?? ""
                                elide: Text.ElideMiddle
                            }

                            OsText {
                                width: parent.width
                                visible: text.length > 0
                                muted: true
                                size: "small"
                                elide: Text.ElideMiddle
                                text: {
                                    const state = view.agentState(rowItem.modelData.state ?? "");
                                    const where = rowItem.modelData.subtitle ?? "";
                                    return state.length > 0 ? (where.length > 0 ? state + " · " + where : state) : where;
                                }
                            }
                        }

                        OsListRow {
                            id: row

                            visible: !rowItem.header
                            width: parent.width
                            focusPolicy: Qt.StrongFocus
                            iconName: rowItem.modelData.kind === "identity" ? "user"
                                    : rowItem.modelData.kind === "knownHost" ? "server" : "key-round"
                            text: {
                                const item = rowItem.modelData;
                                switch (item.kind) {
                                case "identity":
                                case "key":
                                    return item.name;
                                case "agentKey":
                                    return item.comment.length > 0 ? item.comment : item.fingerprint;
                                case "knownHost":
                                    return item.hashed ? qsTr("Hashed host name") : item.hosts;
                                default:
                                    return "";
                                }
                            }
                            subtitle: {
                                const item = rowItem.modelData;
                                switch (item.kind) {
                                case "identity": {
                                    const parts = [];
                                    if (item.user.length > 0)
                                        parts.push(item.user);
                                    if (item.keyName.length > 0)
                                        parts.push(qsTr("key %1").arg(item.keyName));
                                    if (item.hasPassword)
                                        parts.push(qsTr("password saved"));
                                    return parts.join(" · ");
                                }
                                case "key":
                                case "agentKey":
                                    return item.label + (item.bits > 0 && item.label === "RSA" ? " " + item.bits : "") + " · " + item.fingerprint;
                                case "knownHost":
                                    return item.keyType + " · " + item.fingerprint;
                                default:
                                    return "";
                                }
                            }
                            onClicked: {
                                if (rowItem.modelData.kind === "identity")
                                    view.shell.editIdentity(rowItem.modelData.id);
                                else if (rowItem.modelData.kind === "key")
                                    view.openMenu(rowItem.modelData, row);
                            }
                            Keys.onMenuPressed: view.openMenu(rowItem.modelData, row)

                            TapHandler {
                                acceptedButtons: Qt.RightButton
                                onTapped: view.openMenu(rowItem.modelData, row)
                            }

                            OsTag {
                                visible: rowItem.modelData.kind === "key" && (rowItem.modelData.usedBy ?? []).length > 0
                                text: qsTr("Used by %1").arg((rowItem.modelData.usedBy ?? []).join(", "))
                                maxTextWidth: Theme.spacingXxl * 5
                            }

                            OsTag {
                                visible: rowItem.modelData.kind === "key" && rowItem.modelData.hasPrivate === false
                                text: qsTr("Public only")
                            }

                            OsTag {
                                visible: rowItem.modelData.kind === "knownHost" && (rowItem.modelData.marker ?? "").length > 0
                                text: rowItem.modelData.marker === "revoked" ? qsTr("Revoked") : qsTr("Certificate authority")
                            }

                            OsIconButton {
                                visible: rowItem.modelData.kind === "key" || rowItem.modelData.kind === "agentKey"
                                iconName: "copy"
                                toolTip: qsTr("Copy the public key")
                                onClicked: {
                                    if (rowItem.modelData.kind === "key") {
                                        view.copyPublicKey(rowItem.modelData);
                                    } else {
                                        Platform.copyText(rowItem.modelData.publicKey);
                                        Toasts.show(qsTr("Public key copied."), "success");
                                    }
                                }
                            }

                            OsIconButton {
                                visible: rowItem.modelData.kind === "identity" || rowItem.modelData.kind === "key"
                                iconName: "ellipsis"
                                toolTip: qsTr("More actions")
                                onClicked: view.openMenu(rowItem.modelData, row)
                            }
                        }
                    }

                    T.ScrollBar.vertical: OsScrollBar {}
                }

                OsEmptyState {
                    anchors.fill: parent
                    visible: view.sectionEmpty
                    iconName: view.sections.find(entry => entry.id === view.section)?.iconName ?? ""
                    title: view.section === "identities" ? qsTr("No identities yet")
                         : view.section === "keys" ? qsTr("No SSH keys yet")
                         : view.section === "agents" ? qsTr("No agent keys")
                         : qsTr("No known hosts")
                    description: view.section === "identities"
                                 ? qsTr("An identity is a user name with a password and/or a key, ready to give to hosts and groups. Passwords are kept in the encrypted vault.")
                                 : view.section === "keys"
                                   ? qsTr("Generate a new key, or import one made by ssh-keygen or PuTTY. Private keys are kept in the encrypted vault.")
                                   : view.section === "agents"
                                     ? qsTr("No running SSH agent holds keys. OpenSesh asks SSH_AUTH_SOCK on Linux, and the Windows OpenSSH agent and Pageant on Windows.")
                                     : qsTr("~/.ssh/known_hosts is empty or missing.")

                    OsButton {
                        visible: view.section === "identities"
                        variant: "primary"
                        text: qsTr("New identity")
                        iconName: "plus"
                        onClicked: view.shell.newIdentity()
                    }

                    OsButton {
                        visible: view.section === "keys"
                        variant: "primary"
                        text: qsTr("Generate key")
                        iconName: "plus"
                        onClicked: view.shell.generateKey()
                    }

                    OsButton {
                        visible: view.section === "keys"
                        text: qsTr("Import…")
                        iconName: "import"
                        onClicked: view.shell.importKey()
                    }

                    OsButton {
                        visible: view.section === "agents" || view.section === "known"
                        text: qsTr("Refresh")
                        iconName: "refresh-cw"
                        onClicked: view.showSection(view.section)
                    }
                }
            }
        }
    }

    OsContextMenu {
        id: identityMenu

        OsMenuItem {
            text: qsTr("Edit…")
            iconName: "pencil"
            onTriggered: view.shell.editIdentity(view.target.id)
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Delete…")
            iconName: "trash-2"
            onTriggered: view.askDelete(view.target)
        }
    }

    OsContextMenu {
        id: keyMenu

        OsMenuItem {
            text: qsTr("Copy the public key")
            iconName: "copy"
            onTriggered: view.copyPublicKey(view.target)
        }

        OsMenuItem {
            text: qsTr("Export the public key…")
            iconName: "download"
            onTriggered: view.shell.exportKey(view.target.id, "public")
        }

        OsMenuItem {
            text: qsTr("Export the private key…")
            iconName: "download"
            enabled: view.target !== null && view.target.hasPrivate === true
            onTriggered: view.shell.exportKey(view.target.id, "private")
        }

        OsMenuItem {
            text: qsTr("Rename…")
            iconName: "pencil"
            onTriggered: {
                renameField.text = view.target.name;
                renameDialog.open();
                renameField.selectAll();
                renameField.forceActiveFocus();
            }
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Delete…")
            iconName: "trash-2"
            onTriggered: view.askDelete(view.target)
        }
    }

    OsDialog {
        id: renameDialog

        title: qsTr("Rename key")
        acceptText: qsTr("Rename")
        acceptEnabled: renameField.text.trim().length > 0

        onAccepted: {
            KeychainTasks.run(Keychain.renameKey(view.target.id, renameField.text), (code, detail) => {
                if (code.length > 0)
                    Toasts.show(KeychainTasks.message(code, detail), "danger");
            });
        }

        OsTextField {
            id: renameField

            implicitWidth: Math.min(Theme.spacingXxl * 10, renameDialog.maxWidth - renameDialog.leftPadding - renameDialog.rightPadding)
            Accessible.name: qsTr("Name")
            onAccepted: renameDialog.accept()
        }
    }

    OsDialog {
        id: deleteDialog

        readonly property bool isKey: view.target !== null && view.target.kind === "key"

        title: isKey ? qsTr("Delete key?") : qsTr("Delete identity?")
        acceptText: qsTr("Delete")
        dangerous: true

        onAccepted: view.deleteTarget()

        Column {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: view.target === null ? ""
                    : deleteDialog.isKey
                      ? qsTr("“%1” and its private key are deleted from the vault. Servers that trust it keep its public key until you remove it there.").arg(view.target.name)
                      : qsTr("“%1” and its saved password are deleted. Hosts and groups that use it are left without an identity.").arg(view.target.name)
            }
        }
    }
}
