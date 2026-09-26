pragma ComponentBehavior: Bound

// Ctrl+Tab switcher (PLAN Sprint 4): the tabs in most-recently-used order. Ctrl+Tab picks the
// tab used before this one, and while Ctrl stays down a list shows the others: more Tab presses
// (Shift+Tab goes back) or the arrows move through it, and releasing Ctrl switches. Enter and a
// click switch too, Escape cancels. A quick Ctrl+Tab switches at once, without showing the list.
// Ctrl is read from the system (Platform.keyboardModifiers), so releasing it outside the window
// still switches.
//   shell: Item   the AppShell (switcherEntries(), selectTabById())
// Functions: start(step) (1: Ctrl+Tab, -1: Ctrl+Shift+Tab).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.Popup {
    id: control

    required property Item shell
    property var entries: []
    property int currentIndex: 0

    function ctrlHeld() {
        return (Platform.keyboardModifiers() & Qt.ControlModifier) !== 0;
    }

    function start(step) {
        const list = shell.switcherEntries();
        if (list.length < 2)
            return;
        entries = list;
        currentIndex = step > 0 ? 1 : list.length - 1;
        if (!ctrlHeld()) {
            commit();
            return;
        }
        open();
        poll.start();
    }

    function move(step) {
        if (entries.length > 0)
            currentIndex = (currentIndex + step + entries.length) % entries.length;
    }

    function commit() {
        poll.stop();
        const entry = entries[currentIndex];
        close();
        entries = [];
        if (entry)
            shell.selectTabById(entry.tabId);
    }

    parent: T.Overlay.overlay
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: parent ? Math.round(Math.max(0, parent.height - height) / 3) : 0
    width: parent ? Math.max(0, Math.min(Theme.spacingXs * 110, parent.width - 2 * Theme.spacingXl)) : Theme.spacingXs * 110

    implicitHeight: implicitContentHeight + topPadding + bottomPadding

    modal: true
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    padding: Theme.spacingSm

    onOpened: list.forceActiveFocus(Qt.PopupFocusReason)
    onClosed: {
        poll.stop();
        if (control.shell.currentWorkspace && control.shell.currentTab > 0)
            control.shell.currentWorkspace.focusTerminal();
    }

    // Releasing Ctrl anywhere switches.
    Timer {
        id: poll

        interval: 50
        repeat: true
        onTriggered: {
            if (!control.ctrlHeld())
                control.commit();
        }
    }

    contentItem: ListView {
        id: list

        implicitHeight: Math.min(contentHeight, Theme.rowHeight * 10)
        clip: true
        model: control.entries
        currentIndex: control.currentIndex
        boundsBehavior: Flickable.StopAtBounds
        highlightMoveDuration: 0

        Accessible.role: Accessible.List
        Accessible.name: qsTr("Switch tab")

        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

        Keys.onPressed: event => {
            if (event.key === Qt.Key_Tab || event.key === Qt.Key_Down) {
                control.move(1);
            } else if (event.key === Qt.Key_Backtab || event.key === Qt.Key_Up) {
                control.move(-1);
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                control.commit();
            } else {
                return;
            }
            event.accepted = true;
        }
        Keys.onReleased: event => {
            if (event.key === Qt.Key_Control) {
                control.commit();
                event.accepted = true;
            }
        }

        delegate: OsListRow {
            id: row

            required property var modelData
            required property int index
            readonly property int colorIndex: Theme.tabColorNames.indexOf(modelData.color)

            width: ListView.view.width
            text: modelData.title
            iconName: modelData.iconName
            highlighted: index === control.currentIndex
            focusPolicy: Qt.NoFocus

            onClicked: {
                control.currentIndex = index;
                control.commit();
            }

            Rectangle {
                visible: row.colorIndex >= 0
                implicitWidth: Theme.spacingSm
                implicitHeight: Theme.spacingSm
                radius: width / 2
                color: row.colorIndex >= 0 ? Theme.tabColors[row.colorIndex] : Theme.border
            }
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
