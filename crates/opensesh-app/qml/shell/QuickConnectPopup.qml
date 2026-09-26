pragma ComponentBehavior: Bound

// Quick connect (PLAN Sprint 5, Ctrl+Shift+O): a field for user@host:port, ssh://, rdp://,
// serial://..., with what the text means as it is typed, the saved hosts that match and the
// recent targets. Up/Down pick a suggestion; Enter connects in a new tab, Shift+Enter in a split
// to the right, Ctrl+Enter in a split below; Escape closes.
//   shell: Item   the AppShell (connectHost(id, where), connectTarget(text, where))
// Functions: openWith(text).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Popup {
    id: control

    required property Item shell
    readonly property var parsed: JSON.parse(Hosts.parseTarget(field.text) || "{}")
    property var suggestions: []

    function openWith(text) {
        field.text = text ?? "";
        open();
    }

    function refresh() {
        suggestions = JSON.parse(Hosts.suggest(field.text) || "[]");
        list.currentIndex = -1;
    }

    // where: "tab", "right" or "down".
    function go(where) {
        const entry = list.currentIndex >= 0 ? suggestions[list.currentIndex] : null;
        let done = false;
        if (entry && entry.kind === "host")
            done = shell.connectHost(entry.id, where);
        else if (entry && entry.kind === "recent")
            done = shell.connectTarget(entry.text, where);
        else if (field.text.trim().length > 0) {
            // A saved host's exact name wins over the text as an address.
            const id = Hosts.findHost(field.text.trim());
            done = id.length > 0 ? shell.connectHost(id, where) : shell.connectTarget(field.text, where);
        }
        if (done)
            close();
    }

    function whereFor(modifiers) {
        if (modifiers & Qt.ShiftModifier)
            return "right";
        if (modifiers & Qt.ControlModifier)
            return "down";
        return "tab";
    }

    parent: T.Overlay.overlay
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round(Math.min(Theme.spacingXxl * 2, Math.max(0, parent.height - height) / 2)) : 0
    width: parent ? Math.max(0, Math.min(Theme.spacingXs * 140, parent.width - 2 * Theme.spacingXl)) : Theme.spacingXs * 140
    implicitHeight: implicitContentHeight + topPadding + bottomPadding

    modal: true
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    padding: Theme.spacingSm

    readonly property OsFocusReturn focusReturn: OsFocusReturn {
        popup: control
    }

    onAboutToShow: {
        focusReturn.save();
        refresh();
    }
    onOpened: {
        field.forceActiveFocus(Qt.PopupFocusReason);
        field.selectAll();
    }
    onClosed: focusReturn.restore()

    contentItem: Column {
        spacing: Theme.spacingSm

        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Quick connect")

        OsSearchField {
            id: field

            width: parent.width
            placeholderText: qsTr("user@host:port, ssh://…, rdp://…, serial:///dev/ttyUSB0")
            Accessible.name: qsTr("Where to connect")
            Accessible.description: control.parsed.ok ? interpretation.text : (control.parsed.error ?? "")

            onTextChanged: control.refresh()
            Keys.onUpPressed: event => {
                list.currentIndex = Math.max(-1, list.currentIndex - 1);
                event.accepted = true;
            }
            Keys.onDownPressed: event => {
                list.currentIndex = Math.min(list.count - 1, list.currentIndex + 1);
                event.accepted = true;
            }
            Keys.onReturnPressed: event => {
                control.go(control.whereFor(event.modifiers));
                event.accepted = true;
            }
            Keys.onEnterPressed: event => {
                control.go(control.whereFor(event.modifiers));
                event.accepted = true;
            }
        }

        // What the text means, or why it can't be used.
        Row {
            width: parent.width
            spacing: Theme.spacingSm
            visible: field.text.trim().length > 0

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: control.parsed.ok ? "plug-zap" : "circle-alert"
                size: Theme.iconSizeSmall
                color: control.parsed.ok ? Theme.textMuted : Theme.danger
            }

            OsText {
                id: interpretation

                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - Theme.iconSizeSmall - Theme.spacingSm
                size: "small"
                muted: control.parsed.ok === true
                color: control.parsed.ok ? Theme.textMuted : Theme.danger
                elide: Text.ElideRight
                text: {
                    const p = control.parsed;
                    if (!p.ok)
                        return p.error ?? "";
                    const where = (p.user ? p.user + "@" : "") + p.host + (p.port ? ":" + p.port : "");
                    const via = p.jump && p.jump.length > 0 ? qsTr(" through %1").arg(p.jump.join(", ")) : "";
                    if (p.sprint > 0)
                        return qsTr("%1 to %2: arrives in Sprint %3").arg(p.protocol.toUpperCase()).arg(where).arg(p.sprint);
                    return qsTr("%1 to %2%3").arg(p.protocol.toUpperCase()).arg(where).arg(via);
                }
            }
        }

        ListView {
            id: list

            width: parent.width
            height: Math.min(contentHeight, Theme.rowHeight * 8)
            visible: count > 0
            clip: true
            model: control.suggestions
            currentIndex: -1
            boundsBehavior: Flickable.StopAtBounds
            highlightMoveDuration: 0

            Accessible.role: Accessible.List
            Accessible.name: qsTr("Suggestions")

            delegate: OsListRow {
                required property var modelData
                required property int index

                width: ListView.view.width
                text: modelData.kind === "host" ? modelData.name : modelData.text
                subtitle: modelData.kind === "host" ? modelData.target : qsTr("Recent")
                iconName: modelData.kind === "host" ? "server" : "history"
                highlighted: index === list.currentIndex
                focusPolicy: Qt.NoFocus

                onClicked: {
                    list.currentIndex = index;
                    control.go("tab");
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        OsText {
            width: parent.width
            size: "small"
            muted: true
            text: qsTr("Enter: new tab · Shift+Enter: split right · Ctrl+Enter: split down")
            elide: Text.ElideRight
        }
    }

    background: Rectangle {
        color: Theme.surface
        radius: Theme.radiusCard
        border.color: Theme.borderStrong
        border.width: Theme.borderWidth
    }

    T.Overlay.modal: Rectangle {
        color: Theme.scrim
    }
}
