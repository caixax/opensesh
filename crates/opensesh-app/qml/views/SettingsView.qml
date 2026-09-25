// Settings (PLAN §5.4, §6.1): the list of sections on the left and the selected page on the
// right, in a scrolling area. General, Appearance and About work; the other sections say which
// sprint brings them. The section list is one Tab stop: Up/Down and Home/End move between the
// sections, Enter/Return or Space opens the focused one.
//   section: string   id of the selected section: "general", "appearance", "terminal",
//                     "profiles", "themes", "shortcuts", "ssh", "sftp", "security", "data" or
//                     "about" (default "general")
//   sections: var     read-only; [{ id, text, iconName, description?, sprint? }], in list order
// Functions: showSection(id) selects a section (false for an unknown id); smokeSteps() returns
// the functions for SmokeTest.steps that visit every section and open and close the
// "Restore defaults" dialog.
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

FocusScope {
    id: view

    property string section: "general"

    readonly property var sections: [
        {
            id: "general",
            text: qsTr("General"),
            iconName: "sliders-horizontal"
        },
        {
            id: "appearance",
            text: qsTr("Appearance"),
            iconName: "sun-moon"
        },
        {
            id: "terminal",
            text: qsTr("Terminal"),
            iconName: "square-terminal",
            description: qsTr("Font, colors, cursor, scrollback, clipboard and terminal behavior, with a live preview."),
            sprint: 3
        },
        {
            id: "profiles",
            text: qsTr("Profiles"),
            iconName: "user",
            description: qsTr("Terminal profiles that groups, hosts and tabs inherit and override."),
            sprint: 3
        },
        {
            id: "themes",
            text: qsTr("Themes"),
            iconName: "palette",
            description: qsTr("Terminal color themes: the built-in ones, imported ones and your own, with a visual editor."),
            sprint: 3
        },
        {
            id: "shortcuts",
            text: qsTr("Shortcuts"),
            iconName: "keyboard",
            description: qsTr("Every keyboard shortcut, editable, with conflict detection."),
            sprint: 3
        },
        {
            id: "ssh",
            text: qsTr("SSH"),
            iconName: "server",
            description: qsTr("SSH defaults: authentication, agent, keep-alive, algorithms and known hosts."),
            sprint: 7
        },
        {
            id: "sftp",
            text: qsTr("SFTP"),
            iconName: "folder-sync",
            description: qsTr("File transfer defaults: the dual-pane browser, conflicts and permissions."),
            sprint: 8
        },
        {
            id: "security",
            text: qsTr("Security"),
            iconName: "shield-check",
            description: qsTr("The vault: master password, automatic locking and where secrets are kept."),
            sprint: 6
        },
        {
            id: "data",
            text: qsTr("Data & sync"),
            iconName: "refresh-cw",
            description: qsTr("Import from other clients, export your data and sync it between devices."),
            sprint: 16
        },
        {
            id: "about",
            text: qsTr("About"),
            iconName: "info"
        }
    ]
    readonly property int currentIndex: indexOf(section)
    readonly property var currentEntry: sections[Math.max(0, currentIndex)]

    readonly property bool mirrored: LayoutMirroring.enabled
    readonly property real navWidth: Math.round(Math.max(Theme.spacingXxl * 5,
                                                         Math.min(Theme.spacingXxl * 7, width * 0.28)))
    readonly property real pagePadding: width < Theme.spacingXxl * 20 ? Theme.spacingLg : Theme.spacingXl
    readonly property real maxPageWidth: Theme.spacingXxl * 24

    function indexOf(id: string): int {
        for (let i = 0; i < sections.length; ++i) {
            if (sections[i].id === id)
                return i;
        }
        return -1;
    }

    function showSection(id: string): bool {
        if (indexOf(id) < 0) {
            console.warn("SettingsView: unknown section", id);
            return false;
        }
        section = id;
        return true;
    }

    function smokeSteps() {
        const steps = sections.map(entry => () => view.showSection(entry.id));
        steps.push(() => view.showSection("general"));
        steps.push(() => pageLoader.item.openRestoreDialog());
        steps.push(() => pageLoader.item.closeRestoreDialog());
        return steps;
    }

    // Scrolls the page so that the item with the keyboard focus is visible (Tab into a control
    // below the fold).
    function revealFocusedItem() {
        const window = view.Window.window;
        const item = window ? window.activeFocusItem : null;
        if (!item)
            return;
        let ancestor = item.parent;
        while (ancestor && ancestor !== flick.contentItem)
            ancestor = ancestor.parent;
        if (!ancestor)
            return;
        const margin = Theme.spacingLg;
        const top = item.mapToItem(flick.contentItem, 0, 0).y;
        const bottom = top + item.height;
        const maxY = Math.max(0, flick.contentHeight - flick.height);
        if (top - margin < flick.contentY)
            flick.contentY = Math.max(0, top - margin);
        else if (bottom + margin > flick.contentY + flick.height)
            flick.contentY = Math.min(maxY, bottom + margin - flick.height);
    }

    onSectionChanged: flick.contentY = 0

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Settings")

    Rectangle {
        anchors.fill: parent
        color: Theme.bg
    }

    // Section list.
    FocusScope {
        id: nav

        // Entry that is in the Tab chain; follows the selected section until the arrows move it.
        property int focusIndex: -1
        readonly property int tabIndex: focusIndex >= 0 && focusIndex < entries.count
                                        ? focusIndex : Math.max(0, view.currentIndex)

        function moveFocus(index: int) {
            const target = Math.max(0, Math.min(entries.count - 1, index));
            const item = entries.itemAt(target);
            if (!item)
                return;
            focusIndex = target;
            item.forceActiveFocus(Qt.TabFocusReason);
            // Keep the focused entry visible when the list scrolls.
            const top = item.mapToItem(navFlick.contentItem, 0, 0).y;
            if (top < navFlick.contentY)
                navFlick.contentY = top;
            else if (top + item.height > navFlick.contentY + navFlick.height)
                navFlick.contentY = top + item.height - navFlick.height;
        }

        function activate(index: int) {
            const item = entries.itemAt(index);
            if (!item)
                return;
            // A click while the list has the focus moves it to the clicked entry.
            if (!item.activeFocus && nav.activeFocus)
                item.forceActiveFocus(Qt.MouseFocusReason);
            focusIndex = index;
            view.showSection(view.sections[index].id);
        }

        x: view.mirrored ? view.width - width : 0
        width: view.navWidth
        height: view.height
        focus: true

        Accessible.role: Accessible.List
        Accessible.name: qsTr("Settings sections")

        Connections {
            target: view

            function onSectionChanged() {
                nav.focusIndex = -1;
            }
        }

        Keys.onUpPressed: nav.moveFocus(nav.tabIndex - 1)
        Keys.onDownPressed: nav.moveFocus(nav.tabIndex + 1)
        Keys.onPressed: event => {
            switch (event.key) {
            case Qt.Key_Home:
                nav.moveFocus(0);
                break;
            case Qt.Key_End:
                nav.moveFocus(entries.count - 1);
                break;
            case Qt.Key_Return:
            case Qt.Key_Enter:
                nav.activate(nav.tabIndex);
                break;
            default:
                return;
            }
            event.accepted = true;
        }

        // Edge line between the list and the page.
        Rectangle {
            x: view.mirrored ? 0 : parent.width - width
            width: Theme.borderWidth
            height: parent.height
            color: Theme.border
        }

        Flickable {
            id: navFlick

            anchors.fill: parent
            anchors.rightMargin: view.mirrored ? 0 : Theme.borderWidth
            anchors.leftMargin: view.mirrored ? Theme.borderWidth : 0
            contentWidth: width
            contentHeight: navColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick

            Column {
                id: navColumn

                width: navFlick.width
                topPadding: view.pagePadding
                bottomPadding: Theme.spacingLg
                leftPadding: Theme.spacingSm
                rightPadding: Theme.spacingSm
                spacing: Theme.borderWidth * 2

                OsText {
                    width: parent.width - parent.leftPadding - parent.rightPadding
                    leftPadding: Theme.controlPadding
                    rightPadding: Theme.controlPadding
                    bottomPadding: Theme.spacingSm
                    text: qsTr("Settings")
                    size: "large"
                    horizontalAlignment: Text.AlignLeft
                    Accessible.role: Accessible.Heading
                }

                Repeater {
                    id: entries

                    model: view.sections

                    delegate: OsListRow {
                        required property var modelData
                        required property int index

                        width: navColumn.width - navColumn.leftPadding - navColumn.rightPadding
                        text: modelData.text
                        iconName: modelData.iconName
                        selected: modelData.id === view.section
                        focusPolicy: index === nav.tabIndex || activeFocus ? Qt.TabFocus : Qt.NoFocus

                        onClicked: nav.activate(index)
                    }
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }
    }

    // Selected page.
    Flickable {
        id: flick

        x: view.mirrored ? 0 : nav.width
        width: view.width - nav.width
        height: view.height
        contentWidth: width
        contentHeight: pageLoader.height + 2 * view.pagePadding
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick

        Loader {
            id: pageLoader

            x: view.mirrored ? flick.width - width - view.pagePadding : view.pagePadding
            y: view.pagePadding
            width: Math.max(0, Math.min(flick.width - 2 * view.pagePadding, view.maxPageWidth))
            sourceComponent: {
                switch (view.currentEntry.id) {
                case "general":
                    return generalPage;
                case "appearance":
                    return appearancePage;
                case "about":
                    return aboutPage;
                default:
                    return placeholderPage;
                }
            }
        }

        T.ScrollBar.vertical: OsScrollBar {}
    }

    Connections {
        target: view.Window.window

        function onActiveFocusItemChanged() {
            view.revealFocusedItem();
        }
    }

    Component {
        id: generalPage

        SettingsGeneralPage {}
    }

    Component {
        id: appearancePage

        SettingsAppearancePage {}
    }

    Component {
        id: aboutPage

        SettingsAboutPage {}
    }

    Component {
        id: placeholderPage

        SettingsPlaceholderPage {
            iconName: view.currentEntry.iconName
            title: view.currentEntry.text
            description: view.currentEntry.description !== undefined ? view.currentEntry.description : ""
            sprint: view.currentEntry.sprint !== undefined ? view.currentEntry.sprint : 0
            minimumHeight: flick.height - 2 * view.pagePadding
        }
    }
}
