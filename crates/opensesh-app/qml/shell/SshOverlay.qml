pragma ComponentBehavior: Bound

// What the built-in SSH client shows over a terminal pane (Sprint 7). Prompts live in the pane,
// not in dialogs, so several panes can connect at once and each asks in its own place.
//   - While connecting or authenticating: a chip at the top ("Connecting to web (2 of 3)...").
//   - A question: a card in the middle. A new host key shows its fingerprint to trust once or
//     remember; a changed one warns and offers to connect once or replace the saved key.
//     Passwords, key passphrases and keyboard-interactive prompts (one-time codes) have their
//     fields; Escape cancels. What is typed goes straight to the connection and the fields
//     are cleared.
//   - Disconnected: a banner at the bottom with the reason and Reconnect (or, when the vault is
//     locked, Unlock and connect), and the countdown of an automatic reconnection.
//   terminal: TerminalItem  the pane's terminal (its `connection` and `prompt` JSON)
//   connectionText, promptText: string  what is shown (default: the terminal's; screenshots set
//                           samples)
//   shell: Item             the window's AppShell (unlockVault())
//   label: string           what the pane connects to, for texts
//   edgeInset: real         room kept free at the right edge
//   asking: bool            read-only; a question is shown
// Functions: focusPrompt() gives the keyboard to the question, if there is one.
// Signals: answered() after a question was answered (the pane gives the terminal its focus back),
//          closeRequested() for Close on the banner.
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Item {
    id: overlay

    required property TerminalItem terminal
    required property Item shell
    required property string label
    property real edgeInset: 0

    property string connectionText: terminal.connection
    property string promptText: terminal.prompt
    readonly property var connection: parse(connectionText)
    readonly property var prompt: parse(promptText)
    readonly property string phase: connection.state ?? ""
    readonly property bool asking: prompt.kind !== undefined
    readonly property bool busy: !asking && (phase === "connecting" || phase === "authenticating")
    readonly property bool disconnected: !asking && phase === "disconnected"
    // Seconds left before the automatic reconnection (-1: none).
    property int countdown: -1
    // The question on the card (its id; -1 for none).
    property int shownPrompt: -1

    signal answered
    signal closeRequested

    function parse(text) {
        if (text.length === 0)
            return {};
        try {
            return JSON.parse(text);
        } catch (error) {
            return {};
        }
    }

    function focusPrompt() {
        if (prompt.kind !== undefined && promptLoader.item)
            promptLoader.item.takeFocus();
    }

    // action: "trust-once", "trust-save", "submit" or "cancel".
    function answer(action, secrets) {
        const id = prompt.id ?? 0;
        terminal.answerPrompt(id, action, secrets ?? []);
        answered();
    }

    // Enter is what the connection waits for to try again. Not written "\r": qmlcachegen 6.11
    // copies a carriage return into the generated C++ as is, which GCC can't compile.
    function reconnect() {
        terminal.sendText(String.fromCharCode(13));
        terminal.forceActiveFocus(Qt.OtherFocusReason);
    }

    function unlockAndReconnect() {
        shell.unlockVault(() => overlay.reconnect());
    }

    // Handlers read `connection` and `prompt` themselves: the properties built on them may not be
    // up to date yet when a change arrives.
    function showConnection() {
        const down = prompt.kind === undefined && connection.state === "disconnected";
        countdown = down ? (connection.retryIn ?? -1) : -1;
        if (countdown > 0)
            countdownTimer.restart();
        else
            countdownTimer.stop();
    }

    // The card is built again for each question (a wrong password gets empty fields), but not
    // when only the connection's state changed.
    function showPrompt() {
        const question = prompt.kind !== undefined;
        const id = question ? (prompt.id ?? 0) : -1;
        if (id === shownPrompt)
            return;
        // Keep the keyboard where it was: a focused pane hands it to the question.
        const hadFocus = terminal.activeFocus || promptLoader.activeFocus;
        shownPrompt = id;
        promptLoader.active = false;
        switch (prompt.kind) {
        case "hostKey":
            promptLoader.sourceComponent = hostKeyPrompt;
            break;
        case "keyboard":
            promptLoader.sourceComponent = keyboardPrompt;
            break;
        case "password":
        case "passphrase":
            promptLoader.sourceComponent = secretPrompt;
            break;
        default:
            promptLoader.sourceComponent = null;
        }
        promptLoader.active = question;
        if (question && hadFocus)
            Qt.callLater(overlay.focusPrompt);
    }

    onConnectionChanged: showConnection()
    onPromptChanged: {
        showPrompt();
        showConnection();
    }
    Component.onCompleted: {
        showPrompt();
        showConnection();
    }

    Timer {
        id: countdownTimer

        interval: 1000
        repeat: true
        onTriggered: {
            overlay.countdown = Math.max(0, overlay.countdown - 1);
            if (overlay.countdown === 0)
                stop();
        }
    }

    // Connecting or authenticating: a chip at the top.
    Rectangle {
        id: statusChip

        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.topMargin: Theme.spacingLg
        visible: overlay.busy
        width: statusRow.implicitWidth + 2 * Theme.spacingMd
        height: Theme.controlHeightSmall
        radius: height / 2
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: Theme.borderStrong

        Accessible.role: Accessible.StaticText
        Accessible.name: statusText.text

        Row {
            id: statusRow

            anchors.centerIn: parent
            spacing: Theme.spacingSm

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: overlay.phase === "authenticating" ? "key-round" : "plug-zap"
                size: Theme.iconSizeSmall
                color: Theme.accent
            }

            OsText {
                id: statusText

                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, Math.max(0, overlay.width - 4 * Theme.spacingLg - Theme.iconSizeSmall))
                size: "small"
                text: {
                    const connection = overlay.connection;
                    if (overlay.phase === "authenticating")
                        return qsTr("Authenticating as %1…").arg(connection.label ?? "");
                    if ((connection.count ?? 1) > 1)
                        return qsTr("Connecting to %1 (%2 of %3)…").arg(connection.label ?? "").arg((connection.index ?? 0) + 1).arg(connection.count);
                    return qsTr("Connecting to %1…").arg(connection.label ?? "");
                }
            }
        }
    }

    // A question: a card in the middle, over a dimmed terminal.
    Rectangle {
        anchors.fill: parent
        visible: overlay.asking
        color: Theme.bg
        opacity: 0.6

        // The terminal under the card doesn't take clicks meanwhile.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: overlay.focusPrompt()
        }
    }

    OsCard {
        id: promptCard

        anchors.centerIn: parent
        visible: overlay.asking
        width: Math.min(Theme.spacingXxl * 14, overlay.width - 2 * Theme.spacingLg)
        Accessible.role: Accessible.Dialog
        Accessible.name: promptLoader.item ? promptLoader.item.title : ""

        Loader {
            id: promptLoader

            width: parent.width
            active: false
        }
    }

    // Disconnected: the reason, and how to connect again.
    Rectangle {
        id: disconnectBanner

        readonly property bool locked: (overlay.connection.code ?? "") === "locked"

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Theme.spacingLg
        anchors.rightMargin: Theme.spacingLg + overlay.edgeInset
        height: bannerRow.implicitHeight + 2 * Theme.spacingMd
        visible: overlay.disconnected
        radius: Theme.radiusCard
        color: Theme.surface
        border.width: Theme.borderWidth
        border.color: locked ? Theme.warning : Theme.danger

        Accessible.role: Accessible.AlertMessage
        Accessible.name: bannerText.text

        RowLayout {
            id: bannerRow

            anchors.fill: parent
            anchors.leftMargin: Theme.spacingLg
            anchors.rightMargin: Theme.spacingMd
            spacing: Theme.spacingMd

            OsIcon {
                name: disconnectBanner.locked ? "lock" : "unplug"
                color: disconnectBanner.locked ? Theme.warning : Theme.danger
                size: Theme.iconSize
            }

            Column {
                Layout.fillWidth: true
                spacing: Theme.spacingXs

                OsText {
                    id: bannerText

                    width: parent.width
                    text: disconnectBanner.locked ? qsTr("The vault is locked: unlock it to use the password and key saved for %1.").arg(overlay.label)
                                                  : qsTr("Disconnected from %1: %2").arg(overlay.label).arg(overlay.connection.reason ?? "")
                    wrapMode: Text.WordWrap
                    elide: Text.ElideNone
                }

                OsText {
                    width: parent.width
                    visible: overlay.countdown >= 0
                    text: qsTr("Reconnecting in %n s…", "", overlay.countdown)
                    size: "small"
                    muted: true
                }
            }

            OsButton {
                visible: disconnectBanner.locked
                text: qsTr("Unlock and connect")
                iconName: "lock-open"
                variant: "primary"
                onClicked: overlay.unlockAndReconnect()
            }

            OsButton {
                text: qsTr("Reconnect")
                iconName: "refresh-cw"
                variant: disconnectBanner.locked ? "secondary" : "primary"
                onClicked: overlay.reconnect()
            }

            OsButton {
                text: qsTr("Close")
                iconName: "x"
                onClicked: overlay.closeRequested()
            }
        }
    }

    // The fingerprint, selectable so it can be copied and compared.
    component Fingerprint: TextEdit {
        required property string caption
        required property string value

        width: parent ? parent.width : 0
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.WrapAnywhere
        textFormat: TextEdit.PlainText
        text: caption.length > 0 ? qsTr("%1 %2", "a caption, then a key's fingerprint").arg(caption).arg(value) : value
        color: Theme.text
        selectionColor: Theme.selection
        selectedTextColor: Theme.text
        font.family: Theme.monoFontFamily
        font.pixelSize: Theme.fontSizeSmall
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }

    component PromptHeader: RowLayout {
        required property string icon
        required property color tint
        required property string heading

        width: parent ? parent.width : 0
        spacing: Theme.spacingSm

        OsIcon {
            name: parent.icon
            color: parent.tint
            size: Theme.iconSize
        }

        OsText {
            Layout.fillWidth: true
            text: parent.heading
            size: "large"
            wrapMode: Text.WordWrap
            elide: Text.ElideNone
        }
    }

    Component {
        id: hostKeyPrompt

        Column {
            id: hostKey

            readonly property var question: overlay.prompt
            readonly property bool changed: question.changed === true
            readonly property string title: header.heading

            function takeFocus() {
                (changed ? cancelButton : rememberButton).forceActiveFocus(Qt.TabFocusReason);
            }

            spacing: Theme.spacingMd
            Keys.onEscapePressed: overlay.answer("cancel")

            PromptHeader {
                id: header

                icon: hostKey.changed ? "triangle-alert" : "shield-check"
                tint: hostKey.changed ? Theme.danger : Theme.accent
                heading: hostKey.changed ? qsTr("The host key of %1 changed").arg(hostKey.question.host ?? "")
                                         : qsTr("First connection to %1").arg(hostKey.question.host ?? "")
            }

            OsText {
                width: parent.width
                wrapMode: Text.WordWrap
                elide: Text.ElideNone
                text: hostKey.changed ? qsTr("The server's key isn't the one saved in %1 (line %2). Someone may be intercepting the connection, or the server was reinstalled. Don't connect unless you know why it changed.").arg(hostKey.question.file ?? "").arg(hostKey.question.line ?? 0)
                                      : qsTr("OpenSesh doesn't know this server's key yet. Check that its fingerprint is the server's before you trust it.")
            }

            Fingerprint {
                visible: hostKey.changed
                caption: qsTr("Saved:")
                value: hostKey.question.knownFingerprint ?? ""
            }

            Fingerprint {
                caption: hostKey.changed ? qsTr("Now:") : ""
                value: (hostKey.question.keyType ?? "") + " " + (hostKey.question.fingerprint ?? "")
            }

            OsText {
                width: parent.width
                visible: (hostKey.question.otherTypes ?? []).length > 0
                wrapMode: Text.WordWrap
                elide: Text.ElideNone
                size: "small"
                muted: true
                text: qsTr("Keys of other types are known for this host (%1): the server may have a new key.").arg((hostKey.question.otherTypes ?? []).join(", "))
            }

            Flow {
                width: parent.width
                spacing: Theme.spacingSm
                layoutDirection: Qt.RightToLeft

                OsButton {
                    id: rememberButton

                    visible: !hostKey.changed
                    text: qsTr("Trust and remember")
                    variant: "primary"
                    onClicked: overlay.answer("trust-save")
                    Keys.onReturnPressed: overlay.answer("trust-save")
                    Keys.onEnterPressed: overlay.answer("trust-save")
                }

                OsButton {
                    id: cancelButton

                    text: hostKey.changed ? qsTr("Don't connect") : qsTr("Cancel")
                    variant: hostKey.changed ? "primary" : "secondary"
                    onClicked: overlay.answer("cancel")
                    Keys.onReturnPressed: overlay.answer("cancel")
                    Keys.onEnterPressed: overlay.answer("cancel")
                }

                OsButton {
                    text: qsTr("Connect once")
                    variant: hostKey.changed ? "secondary" : "ghost"
                    onClicked: overlay.answer("trust-once")
                    Keys.onReturnPressed: overlay.answer("trust-once")
                    Keys.onEnterPressed: overlay.answer("trust-once")
                }

                OsButton {
                    visible: hostKey.changed
                    text: qsTr("Replace the saved key")
                    variant: "danger"
                    onClicked: overlay.answer("trust-save")
                    Keys.onReturnPressed: overlay.answer("trust-save")
                    Keys.onEnterPressed: overlay.answer("trust-save")
                }
            }
        }
    }

    // A password or a key file's passphrase.
    Component {
        id: secretPrompt

        Column {
            id: secret

            readonly property var question: overlay.prompt
            readonly property bool passphrase: question.kind === "passphrase"
            readonly property string title: header.heading

            function takeFocus() {
                field.forceActiveFocus(Qt.TabFocusReason);
            }

            function submit() {
                if (field.text.length === 0)
                    return;
                const value = field.text;
                field.text = "";
                overlay.answer("submit", [value]);
            }

            spacing: Theme.spacingMd
            Keys.onEscapePressed: overlay.answer("cancel")

            PromptHeader {
                id: header

                icon: secret.passphrase ? "key-round" : "lock"
                tint: Theme.accent
                heading: secret.passphrase ? qsTr("Passphrase for a key") : qsTr("Password for %1").arg(secret.question.target ?? "")
            }

            OsText {
                width: parent.width
                visible: secret.passphrase
                wrapMode: Text.WrapAnywhere
                elide: Text.ElideNone
                muted: true
                text: secret.question.key ?? ""
            }

            OsPasswordField {
                id: field

                width: parent.width
                placeholderText: secret.passphrase ? qsTr("Passphrase") : qsTr("Password")
                error: secret.question.retry === true
                onAccepted: secret.submit()
            }

            OsText {
                width: parent.width
                visible: secret.question.retry === true
                wrapMode: Text.WordWrap
                elide: Text.ElideNone
                size: "small"
                color: Theme.danger
                text: secret.passphrase ? qsTr("Wrong passphrase. Try again.") : qsTr("Wrong password. Try again.")
            }

            Flow {
                width: parent.width
                spacing: Theme.spacingSm
                layoutDirection: Qt.RightToLeft

                OsButton {
                    text: secret.passphrase ? qsTr("Use the key") : qsTr("Connect")
                    variant: "primary"
                    enabled: field.text.length > 0
                    onClicked: secret.submit()
                }

                OsButton {
                    text: secret.passphrase ? qsTr("Skip this key") : qsTr("Cancel")
                    onClicked: overlay.answer("cancel")
                }
            }
        }
    }

    // Keyboard-interactive: the server's questions (a one-time code, a password...).
    Component {
        id: keyboardPrompt

        Column {
            id: keyboard

            readonly property var question: overlay.prompt
            readonly property var fields: question.fields ?? []
            readonly property string title: header.heading

            function takeFocus() {
                const first = answers.itemAt(0);
                if (first)
                    first.field.forceActiveFocus(Qt.TabFocusReason);
                else
                    continueButton.forceActiveFocus(Qt.TabFocusReason);
            }

            function submit() {
                const values = [];
                for (let i = 0; i < answers.count; ++i) {
                    const item = answers.itemAt(i);
                    values.push(item ? item.field.text : "");
                    if (item)
                        item.field.text = "";
                }
                overlay.answer("submit", values);
            }

            // Enter in a field moves to the next one, and answers from the last.
            function next(index) {
                const item = answers.itemAt(index + 1);
                if (item)
                    item.field.forceActiveFocus(Qt.TabFocusReason);
                else
                    submit();
            }

            spacing: Theme.spacingMd
            Keys.onEscapePressed: overlay.answer("cancel")

            PromptHeader {
                id: header

                icon: "keyboard"
                tint: Theme.accent
                heading: (keyboard.question.name ?? "").length > 0 ? keyboard.question.name
                                                                     : qsTr("Verification for %1").arg(keyboard.question.target ?? "")
            }

            OsText {
                width: parent.width
                visible: text.length > 0
                wrapMode: Text.WordWrap
                elide: Text.ElideNone
                text: keyboard.question.instructions ?? ""
            }

            Repeater {
                id: answers

                model: keyboard.fields

                delegate: Column {
                    id: answer

                    required property var modelData
                    required property int index
                    readonly property Item field: modelData.echo ? plainField : hiddenField

                    width: keyboard.width
                    spacing: Theme.spacingXs

                    OsText {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        elide: Text.ElideNone
                        text: answer.modelData.label
                    }

                    OsTextField {
                        id: plainField

                        width: parent.width
                        visible: answer.modelData.echo === true
                        Accessible.name: answer.modelData.label
                        onAccepted: keyboard.next(answer.index)
                    }

                    OsPasswordField {
                        id: hiddenField

                        width: parent.width
                        visible: answer.modelData.echo !== true
                        placeholderText: ""
                        Accessible.name: answer.modelData.label
                        onAccepted: keyboard.next(answer.index)
                    }
                }
            }

            Flow {
                width: parent.width
                spacing: Theme.spacingSm
                layoutDirection: Qt.RightToLeft

                OsButton {
                    id: continueButton

                    text: qsTr("Continue")
                    variant: "primary"
                    onClicked: keyboard.submit()
                    Keys.onReturnPressed: keyboard.submit()
                    Keys.onEnterPressed: keyboard.submit()
                }

                OsButton {
                    text: qsTr("Cancel")
                    onClicked: overlay.answer("cancel")
                }
            }
        }
    }
}
