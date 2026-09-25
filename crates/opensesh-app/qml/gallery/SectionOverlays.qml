// Gallery section for the overlay components: OsDialog, OsDrawer, OsContextMenu with OsMenuItem
// and OsMenuSeparator, OsCommandPalette (over sample actions), OsTooltip, OsToast and OsToastHost.
// Buttons open the real popups; static
// previews show menu items, a tooltip and toasts in their states. The toast buttons go through
// the `Toasts` singleton, so the window needs an OsToastHost to show them.
//   pinTooltip: bool     keep the preview tooltip open while the section is visible (default
//                        true); it lives in the window overlay, so it is also hidden while its
//                        button is outside visibleTop..visibleBottom
//   visibleTop, visibleBottom: real  part of the section on screen, in section coordinates
//                        (bind them to the scroll position; default: the whole section)
//   lastResult: string   read-only; what the last dialog or menu interaction did
//   smokeSteps: list     functions that open and close every overlay, for SmokeTest.steps
// Functions (for screenshot `prepare` hooks): openMenu(showSubMenu), openMenuAt(item), openDialog(),
// openDrawer(), openPalette(query), showSampleToasts(), closeAll().
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Column {
    id: section

    property bool pinTooltip: true
    property real visibleTop: 0
    property real visibleBottom: height
    readonly property string lastResult: resultText.result
    readonly property var smokeSteps: [
        () => dialog.open(),
        () => dialog.close(),
        () => dangerDialog.open(),
        () => dangerDialog.close(),
        () => rightDrawer.open(),
        () => rightDrawer.close(),
        () => leftDrawer.open(),
        () => leftDrawer.close(),
        () => section.openMenu(true),
        () => contextMenu.dismiss(),
        () => section.openPalette(""),
        () => palette.setQuery("conn"),
        () => {
            if (palette.resultCount < 1)
                console.warn("gallery: the palette found no sample action for \"conn\"");
            palette.runCurrent();
        },
        () => section.showSampleToasts()
    ]

    function openMenuAt(item) {
        contextMenu.popup(item, 0, item.height + Theme.spacingXs);
    }

    // With `showSubMenu`, also opens the "Open with" submenu (as a click on its item does).
    function openMenu(showSubMenu) {
        openMenuAt(menuButton);
        if (!showSubMenu)
            return;
        for (let i = 0; i < contextMenu.count; ++i) {
            const item = contextMenu.itemAt(i) as OsMenuItem;
            if (item && item.subMenu) {
                contextMenu.currentIndex = i;
                item.triggered();
                return;
            }
        }
    }

    function openDialog() {
        dialog.open();
    }

    function openDrawer() {
        rightDrawer.open();
    }

    function openPalette(query) {
        palette.openWith(query);
    }

    function closeAll() {
        for (const popup of [dialog, dangerDialog, rightDrawer, leftDrawer, palette])
            popup.close();
        contextMenu.dismiss();
    }

    function showSampleToasts() {
        Toasts.show(qsTr("Connected to deploy@web-01."), "info");
        Toasts.show(qsTr("Settings saved."), "success");
        Toasts.show(qsTr("The host key changed since the last connection. Verify it before you continue."),
                    "warning");
        Toasts.show(qsTr("Connection to db-02 lost."), "danger", qsTr("Retry"), toastAction.actionId);
    }

    spacing: Theme.spacingXl

    OsAction {
        id: toastAction

        actionId: "gallery.toastAction"
        text: qsTr("Gallery toast action")
        category: qsTr("Gallery")
        showInPalette: false
        onTriggered: resultText.result = qsTr("Toast action triggered")
    }

    // Sample actions for the command palette (the shell's own actions don't exist in the gallery).
    readonly property list<OsAction> sampleActions: [
        OsAction {
            actionId: "gallery.quickConnect"
            text: qsTr("Quick connect")
            category: qsTr("Connections")
            iconName: "zap"
            shortcut: "Ctrl+Shift+O"
            onTriggered: resultText.result = qsTr("Ran \"%1\"").arg(text)
        },
        OsAction {
            actionId: "gallery.newTab"
            text: qsTr("New local terminal")
            category: qsTr("Tabs")
            iconName: "square-terminal"
            shortcut: "Ctrl+Shift+T"
            onTriggered: resultText.result = qsTr("Ran \"%1\"").arg(text)
        },
        OsAction {
            actionId: "gallery.sftp"
            text: qsTr("Toggle SFTP panel")
            category: qsTr("View")
            iconName: "folder-sync"
            shortcut: "Ctrl+Shift+E"
            onTriggered: resultText.result = qsTr("Ran \"%1\"").arg(text)
        },
        OsAction {
            actionId: "gallery.reconnect"
            text: qsTr("Reconnect all sessions")
            category: qsTr("Connections")
            iconName: "refresh-cw"
            onTriggered: resultText.result = qsTr("Ran \"%1\"").arg(text)
        },
        OsAction {
            actionId: "gallery.settings"
            text: qsTr("Open settings")
            category: qsTr("App")
            iconName: "settings"
            shortcut: "Ctrl+,"
            onTriggered: resultText.result = qsTr("Ran \"%1\"").arg(text)
        },
        OsAction {
            actionId: "gallery.disabled"
            text: qsTr("Disconnect (no session)")
            category: qsTr("Connections")
            iconName: "unplug"
            enabled: false
        }
    ]

    Component.onCompleted: {
        for (const action of [toastAction, ...sampleActions]) {
            if (!ActionRegistry.find(action.actionId))
                ActionRegistry.register(action);
        }
    }
    Component.onDestruction: {
        for (const action of [toastAction, ...sampleActions])
            ActionRegistry.unregister(action);
    }

    Column {
        width: parent.width
        spacing: Theme.spacingSm

        OsText {
            text: qsTr("Overlays")
            size: "title"
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            text: qsTr("Dialogs, drawers, menus, tooltips and toasts. The buttons open the real popups; the previews show them in place.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    // Dialogs and drawers.
    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Dialogs and drawers")
            description: qsTr("Modal, over a scrim. Escape, the close button or the reject button dismiss them.")
        }

        Row {
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Open dialog")
                variant: "primary"
                onClicked: dialog.open()
            }

            OsButton {
                text: qsTr("Open destructive dialog")
                iconName: "trash-2"
                onClicked: dangerDialog.open()
            }

            OsButton {
                text: qsTr("Open drawer (right)")
                iconName: "panel-right"
                onClicked: rightDrawer.open()
            }

            OsButton {
                text: qsTr("Open drawer (left)")
                iconName: "panel-left"
                onClicked: leftDrawer.open()
            }
        }

        OsText {
            id: resultText

            property string result: qsTr("None yet")

            text: qsTr("Last result: %1").arg(result)
            muted: true
        }
    }

    // Menus.
    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Context menu")
            description: qsTr("Live menu with icons, shortcuts, a checkable item, a submenu and a disabled item; static items on the right.")
        }

        Row {
            spacing: Theme.spacingXl

            Column {
                spacing: Theme.spacingSm

                OsButton {
                    id: menuButton

                    text: qsTr("Open menu")
                    iconName: "ellipsis"
                    onClicked: section.openMenuAt(menuButton)
                }

                Rectangle {
                    id: menuArea

                    width: Theme.spacingXs * 60
                    height: Theme.spacingXs * 24
                    radius: Theme.radiusCard
                    color: Theme.surface
                    border.color: Theme.border
                    border.width: Theme.borderWidth
                    activeFocusOnTab: true

                    Accessible.role: Accessible.Pane
                    Accessible.name: qsTr("Context menu area")
                    Accessible.description: qsTr("Right-click, or press the Menu key or Shift+F10")

                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Menu
                                || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))) {
                            contextMenu.popup(menuArea, menuArea.width / 2, menuArea.height / 2);
                            event.accepted = true;
                        }
                    }

                    OsText {
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.spacingLg
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        text: qsTr("Right-click here, or focus this area and press the Menu key")
                        muted: true
                        size: "small"
                    }

                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: contextMenu.popup()
                    }

                    OsFocusRing {
                        target: menuArea
                        baseRadius: Theme.radiusCard
                    }
                }
            }

            // Static preview: the same items outside a popup, in their states.
            Rectangle {
                width: menuPreview.width + 2 * Theme.spacingXs
                height: menuPreview.height + 2 * Theme.spacingXs
                color: Theme.surface
                radius: Theme.radiusControl
                border.color: Theme.borderStrong
                border.width: Theme.borderWidth

                Column {
                    id: menuPreview

                    x: Theme.spacingXs
                    y: Theme.spacingXs
                    width: {
                        let widest = Theme.spacingXs * 45;
                        for (const child of children)
                            widest = Math.max(widest, child.implicitWidth);
                        return widest;
                    }

                    OsMenuItem {
                        width: parent.width
                        text: qsTr("Copy")
                        iconName: "copy"
                        shortcutText: Platform.keySequenceText(Qt.Key_C, Qt.ControlModifier)
                        reserveLeading: true
                    }

                    OsMenuItem {
                        width: parent.width
                        text: qsTr("Rename (highlighted)")
                        iconName: "pencil"
                        shortcutText: Platform.keySequenceText(Qt.Key_F2, Qt.NoModifier)
                        reserveLeading: true
                        highlighted: true
                    }

                    OsMenuItem {
                        width: parent.width
                        text: qsTr("Paste (disabled)")
                        shortcutText: Platform.keySequenceText(Qt.Key_V, Qt.ControlModifier)
                        reserveLeading: true
                        enabled: false
                    }

                    OsMenuSeparator {
                        width: parent.width
                    }

                    OsMenuItem {
                        width: parent.width
                        text: qsTr("Show hidden files")
                        checkable: true
                        checked: true
                        reserveLeading: true
                    }

                    OsMenuItem {
                        width: parent.width
                        text: qsTr("Delete")
                        iconName: "trash-2"
                        shortcutText: Platform.keySequenceText(Qt.Key_Delete, Qt.NoModifier)
                        reserveLeading: true
                    }
                }
            }
        }
    }

    // Command palette.
    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Command palette")
            description: qsTr("Fuzzy search over the action registry, here with sample actions. Disabled actions are not listed; Enter runs the selected one.")
        }

        Row {
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Open command palette")
                iconName: "command"
                onClicked: section.openPalette("")
            }

            OsButton {
                text: qsTr("Open with \"conn\"")
                iconName: "search"
                onClicked: section.openPalette("conn")
            }
        }
    }

    // Tooltips.
    Column {
        id: tooltipGroup

        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Tooltip")
            description: qsTr("Shown after 600 ms of hover and hidden after 5 s. The second one stays open.")
        }

        Row {
            id: tooltipRow

            spacing: Theme.spacingSm

            OsButton {
                id: hoverButton

                text: qsTr("Hover me")

                OsTooltip {
                    visible: hoverButton.hovered
                    text: qsTr("Tooltips explain a control in a few words.")
                }
            }

            OsButton {
                id: pinnedButton

                text: qsTr("Pinned tooltip")
                iconName: "info"

                OsTooltip {
                    // The button's top edge in section coordinates.
                    readonly property real top: tooltipGroup.y + tooltipRow.y + pinnedButton.y

                    x: pinnedButton.width + Theme.spacingSm
                    y: Math.round((pinnedButton.height - height) / 2)
                    visible: section.pinTooltip && section.visible && top >= section.visibleTop
                             && top + pinnedButton.height <= section.visibleBottom
                    delay: 0
                    timeout: -1
                    closePolicy: T.Popup.NoAutoClose
                    text: qsTr("Always visible: surface2, a strong border and small text.")
                }
            }
        }
    }

    // Toasts.
    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Toasts")
            description: qsTr("Bottom-right stack, at most four, dismissed after 5 s unless hovered.")
        }

        Row {
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Info toast")
                iconName: "info"
                onClicked: Toasts.show(qsTr("Connected to deploy@web-01."), "info")
            }

            OsButton {
                text: qsTr("Success toast")
                iconName: "circle-check"
                onClicked: Toasts.show(qsTr("Settings saved."), "success")
            }

            OsButton {
                text: qsTr("Warning toast")
                iconName: "triangle-alert"
                onClicked: Toasts.show(qsTr("The host key changed since the last connection. Verify it before you continue."),
                                       "warning")
            }

            OsButton {
                text: qsTr("Danger toast with action")
                iconName: "circle-x"
                onClicked: Toasts.show(qsTr("Connection to db-02 lost."), "danger", qsTr("Retry"),
                                       toastAction.actionId)
            }
        }

        // Static preview.
        Column {
            spacing: Theme.spacingSm

            OsToast {
                kind: "info"
                text: qsTr("Connected to deploy@web-01.")
            }

            OsToast {
                kind: "success"
                text: qsTr("Settings saved.")
                actionText: qsTr("Undo")
            }

            OsToast {
                kind: "warning"
                text: qsTr("The host key changed since the last connection. Verify it before you continue.")
                actionText: qsTr("Details")
            }

            OsToast {
                kind: "danger"
                text: qsTr("Connection to db-02 lost.")
                actionText: qsTr("Retry")
            }
        }
    }

    OsCommandPalette {
        id: palette
    }

    OsDialog {
        id: dialog

        title: qsTr("Rename host")
        acceptText: qsTr("Rename")
        onAccepted: resultText.result = qsTr("Dialog accepted")
        onRejected: resultText.result = qsTr("Dialog rejected")

        Column {
            width: Theme.spacingXs * 88
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("Dialogs hold one short, focused task. The body can be any content; the footer has a ghost reject button and a primary accept button.")
            }

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                muted: true
                size: "small"
                text: qsTr("Press Escape to cancel.")
            }
        }
    }

    OsDialog {
        id: dangerDialog

        title: qsTr("Delete 3 hosts?")
        acceptText: qsTr("Delete")
        dangerous: true
        onAccepted: resultText.result = qsTr("Hosts deleted (not really)")
        onRejected: resultText.result = qsTr("Deletion cancelled")

        Column {
            width: Theme.spacingXs * 88

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("This removes the hosts and their saved sessions. It cannot be undone.")
            }
        }
    }

    OsDrawer {
        id: rightDrawer

        title: qsTr("Host details")
        edge: Qt.RightEdge

        Column {
            width: parent.width
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("Drawers are side sheets for secondary content that keeps the main view in context.")
            }

            OsText {
                text: qsTr("deploy@web-01")
                muted: true
            }

            OsButton {
                text: qsTr("Close drawer")
                onClicked: rightDrawer.close()
            }
        }
    }

    OsDrawer {
        id: leftDrawer

        title: qsTr("Navigation")
        edge: Qt.LeftEdge

        Column {
            width: parent.width
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("The same sheet from the left edge; the border moves to the inner side.")
            }

            OsButton {
                text: qsTr("Close drawer")
                onClicked: leftDrawer.close()
            }
        }
    }

    OsContextMenu {
        id: contextMenu

        OsMenuItem {
            text: qsTr("Copy")
            iconName: "copy"
            shortcutText: Platform.keySequenceText(Qt.Key_C, Qt.ControlModifier)
            onTriggered: resultText.result = qsTr("Menu: Copy")
        }

        OsMenuItem {
            text: qsTr("Paste")
            shortcutText: Platform.keySequenceText(Qt.Key_V, Qt.ControlModifier)
            enabled: false
        }

        OsMenuItem {
            text: qsTr("Rename")
            iconName: "pencil"
            shortcutText: Platform.keySequenceText(Qt.Key_F2, Qt.NoModifier)
            onTriggered: resultText.result = qsTr("Menu: Rename")
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Show hidden files")
            checkable: true
            checked: true
            onToggled: resultText.result = checked ? qsTr("Menu: hidden files shown") : qsTr("Menu: hidden files hidden")
        }

        OsContextMenu {
            title: qsTr("Open with")

            OsMenuItem {
                text: qsTr("Terminal")
                iconName: "terminal"
                onTriggered: resultText.result = qsTr("Menu: Open with Terminal")
            }

            OsMenuItem {
                text: qsTr("File manager")
                iconName: "folder-open"
                onTriggered: resultText.result = qsTr("Menu: Open with File manager")
            }
        }

        OsMenuSeparator {}

        OsMenuItem {
            text: qsTr("Delete")
            iconName: "trash-2"
            shortcutText: Platform.keySequenceText(Qt.Key_Delete, Qt.NoModifier)
            onTriggered: resultText.result = qsTr("Menu: Delete")
        }
    }
}
