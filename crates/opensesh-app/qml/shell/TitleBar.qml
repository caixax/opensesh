pragma ComponentBehavior: Bound

// Title bar row (PLAN §5.3): logo and name, the session tabs (unless they sit in their own row),
// a free area to move the window, the side panel toggle, the command palette button and, with
// custom decorations, the window buttons. With a frameless window, dragging any free area moves
// the window (startSystemMove, which works on Wayland) and a double click toggles maximize.
//   window: Window        the window
//   shell: Item           the AppShell (tab state, side panel state, shortcut texts)
//   showTabs: bool        show the session tabs in this row
//   frameless: bool       the window has no system frame, so this row moves it
//   showWindowButtons: bool
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Rectangle {
    id: bar

    required property Window window
    required property Item shell
    property bool showTabs: true
    property bool frameless: true
    property bool showWindowButtons: true

    readonly property string paletteShortcut: shell.shortcutText("app.commandPalette")
    // Narrow windows keep the room for the tabs: no wordmark, and an icon-only palette button.
    readonly property bool narrow: width < Theme.spacingXxl * 30

    implicitHeight: Theme.titleBarHeight
    color: Theme.bg

    // Only the free areas reach these: the controls on top take their own presses.
    DragHandler {
        target: null
        enabled: bar.frameless
        onActiveChanged: {
            if (active)
                bar.window.startSystemMove();
        }
    }

    TapHandler {
        enabled: bar.frameless
        onTapped: {
            if (tapCount === 2)
                bar.shell.toggleMaximize();
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacingMd
        spacing: Theme.spacingSm

        Image {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Theme.iconSize
            Layout.preferredHeight: Theme.iconSize
            source: "qrc:/qt/qml/cc/caixa/opensesh/data/icons/cc.caixa.OpenSesh.svg"
            sourceSize: Qt.size(Math.round(Theme.iconSize), Math.round(Theme.iconSize))
            fillMode: Image.PreserveAspectFit
            smooth: true
            Accessible.ignored: true
        }

        OsText {
            Layout.alignment: Qt.AlignVCenter
            Layout.rightMargin: Theme.spacingSm
            visible: !bar.narrow
            text: qsTr("OpenSesh")
            font.weight: Font.DemiBold
        }

        Loader {
            id: tabsLoader

            Layout.alignment: Qt.AlignVCenter
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.maximumWidth: item ? item.implicitWidth : 0
            Layout.preferredWidth: item ? item.implicitWidth : 0
            Layout.preferredHeight: item ? item.implicitHeight : 0
            active: bar.showTabs
            visible: active

            sourceComponent: SessionTabStrip {
                shell: bar.shell
                newTabShortcut: bar.shell.shortcutText("tab.newLocal")
            }
        }

        // Free area: moves the window (handlers above).
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: Theme.spacingXxl * 2
        }

        OsIconButton {
            Layout.alignment: Qt.AlignVCenter
            implicitWidth: Theme.controlHeightSmall
            implicitHeight: Theme.controlHeightSmall
            // A detached window has no side panel.
            visible: !bar.shell.detached
            iconName: bar.shell.sidePanelLeft ? "panel-left" : "panel-right"
            checked: bar.shell.sidePanelOpen
            toolTip: qsTr("Side panel (%1)").arg(bar.shell.shortcutText("view.sidePanel"))
            Accessible.checkable: true
            Accessible.checked: checked
            onClicked: ActionRegistry.trigger("view.sidePanel")
        }

        OsButton {
            id: paletteButton

            Layout.alignment: Qt.AlignVCenter
            Layout.rightMargin: bar.showWindowButtons ? 0 : Theme.spacingSm
            implicitWidth: bar.narrow ? Theme.controlHeightSmall : implicitContentWidth + leftPadding + rightPadding
            implicitHeight: Theme.controlHeightSmall
            leftPadding: bar.narrow ? 0 : Theme.spacingSm
            rightPadding: bar.narrow ? 0 : Theme.spacingMd
            iconName: "search"
            text: bar.narrow ? "" : bar.paletteShortcut
            Accessible.name: qsTr("Command palette")
            Accessible.description: qsTr("Shortcut: %1").arg(bar.paletteShortcut)
            onClicked: ActionRegistry.trigger("app.commandPalette")

            OsTooltip {
                visible: paletteButton.hovered
                text: qsTr("Command palette (%1)").arg(bar.paletteShortcut)
            }
        }

        WindowButtons {
            Layout.alignment: Qt.AlignTop
            visible: bar.showWindowButtons
            window: bar.window
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: Theme.borderWidth
        color: Theme.border
    }
}
