// Gallery section: structure and navigation components (tabs, rail, cards, list rows, tree,
// tags, badges, section headers, form rows, empty state, splitter, progress) in their states.
// Give it a width; the height is implicit.
// Functions: showFocus()  puts keyboard focus on the second rail entry (screenshots).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: section

    // Two demo blocks per row when they can be at least 384 px (at scale 1) wide, else one.
    readonly property real blockWidth: width >= Theme.spacingXxl * 24 + Theme.spacingXl
                                       ? Math.floor((width - Theme.spacingXl) / 2) : width

    implicitWidth: Theme.spacingXxl * 34
    implicitHeight: content.implicitHeight

    // Shows the keyboard focus ring on the second rail entry.
    function showFocus() {
        iconRail.moveFocus(1, 1);
    }

    // A titled demo block; `caption` lists the component and the states shown.
    component Demo: Column {
        id: demo

        property string title
        property string caption

        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: demo.title
            description: demo.caption
        }
    }

    component Caption: OsText {
        size: "small"
        muted: true
    }

    Column {
        id: content

        width: parent.width
        spacing: Theme.spacingXl

        Column {
            width: parent.width
            spacing: Theme.spacingSm

            OsText {
                text: qsTr("Structure and navigation")
                size: "title"
                Accessible.role: Accessible.Heading
            }

            OsText {
                width: parent.width
                text: qsTr("Tabs, rail, cards, lists, tree, tags, badges, headers, forms, empty states, splitter and progress.")
                muted: true
                wrapMode: Text.Wrap
                elide: Text.ElideNone
            }
        }

        // Tabs.
        Demo {
            width: parent.width
            title: qsTr("Tabs")
            caption: qsTr("OsTabBar: current, activity dot, elided title, not closable, disabled")

            Rectangle {
                width: parent.width
                height: Theme.titleBarHeight
                color: Theme.bg
                border.width: Theme.borderWidth
                border.color: Theme.border
                radius: Theme.radiusControl

                OsTabBar {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: Theme.spacingSm
                    anchors.rightMargin: Theme.spacingSm
                    anchors.verticalCenter: parent.verticalCenter
                    currentIndex: 0

                    OsTabButton {
                        text: qsTr("prod-db-01")
                        iconName: "server"
                    }
                    OsTabButton {
                        text: qsTr("build-runner")
                        iconName: "terminal"
                        activity: true
                    }
                    OsTabButton {
                        text: qsTr("A very long session title that does not fit in a tab")
                        iconName: "folder"
                    }
                    OsTabButton {
                        text: qsTr("Local shell")
                        iconName: "square-terminal"
                        closable: false
                    }
                    OsTabButton {
                        text: qsTr("Offline host")
                        iconName: "unplug"
                        enabled: false
                    }
                }
            }
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingXl

            // Rail.
            Demo {
                width: section.blockWidth
                title: qsTr("Rail")
                caption: qsTr("OsRail: icons (keyboard focus on Hosts) and labels")

                Row {
                    // Room for the label tooltip of the focused icon-only entry.
                    spacing: Theme.spacingXxl * 2.5

                    Rectangle {
                        width: iconRail.width + 2 * Theme.borderWidth
                        height: Theme.spacingXxl * 10
                        color: "transparent"
                        border.width: Theme.borderWidth
                        border.color: Theme.border

                        OsRail {
                            id: iconRail

                            x: Theme.borderWidth
                            y: Theme.borderWidth
                            height: parent.height - 2 * Theme.borderWidth
                            currentId: "sessions"
                            model: [
                                { id: "sessions", text: qsTr("Sessions"), iconName: "terminal" },
                                { id: "hosts", text: qsTr("Hosts"), iconName: "server" },
                                { id: "files", text: qsTr("Files"), iconName: "folder" },
                                { id: "keys", text: qsTr("Keys"), iconName: "key-round", enabled: false }
                            ]
                            footerModel: [
                                { id: "settings", text: qsTr("Settings"), iconName: "settings" }
                            ]
                        }
                    }

                    Rectangle {
                        width: labelRail.width + 2 * Theme.borderWidth
                        height: Theme.spacingXxl * 10
                        color: "transparent"
                        border.width: Theme.borderWidth
                        border.color: Theme.border

                        OsRail {
                            id: labelRail

                            x: Theme.borderWidth
                            y: Theme.borderWidth
                            height: parent.height - 2 * Theme.borderWidth
                            showLabels: true
                            currentId: "files"
                            model: iconRail.model
                            footerModel: iconRail.footerModel
                        }
                    }
                }
            }

            // Tree.
            Demo {
                width: section.blockWidth
                title: qsTr("Tree")
                caption: qsTr("OsTreeView: expanded, collapsed, current row")

                Rectangle {
                    width: parent.width
                    height: Theme.spacingXxl * 10
                    color: Theme.surface
                    radius: Theme.radiusCard
                    border.width: Theme.borderWidth
                    border.color: Theme.border

                    OsTreeView {
                        id: tree

                        anchors.fill: parent
                        anchors.margins: Theme.spacingXs
                        Accessible.name: qsTr("Hosts")
                        currentId: "web-2"
                        nodes: [
                            { id: "prod", text: qsTr("Production"), iconName: "folder", children: [
                                    { id: "web", text: qsTr("Web servers"), iconName: "folder", children: [
                                            { id: "web-1", text: qsTr("web-01.example.org"), iconName: "server" },
                                            { id: "web-2", text: qsTr("web-02.example.org"), iconName: "server" }
                                        ] },
                                    { id: "db", text: qsTr("db-01.example.org"), iconName: "server" }
                                ] },
                            { id: "staging", text: qsTr("Staging"), iconName: "folder", children: [
                                    { id: "stg-1", text: qsTr("stg-01"), iconName: "server" }
                                ] },
                            { id: "local", text: qsTr("Local shell"), iconName: "square-terminal" },
                            { id: "serial", text: qsTr("Serial console on a very long device name"), iconName: "usb" }
                        ]
                        Component.onCompleted: {
                            expand("prod");
                            expand("web");
                        }
                    }
                }
            }

            // List rows.
            Demo {
                width: section.blockWidth
                title: qsTr("List rows")
                caption: qsTr("OsListRow: plain, subtitle, trailing text and slot, selected, highlighted, disabled")

                Column {
                    width: parent.width
                    spacing: Theme.spacingXs

                    OsListRow {
                        width: parent.width
                        text: qsTr("Plain row")
                    }
                    OsListRow {
                        width: parent.width
                        text: qsTr("prod-db-01")
                        subtitle: qsTr("admin@10.0.0.12:22")
                        iconName: "server"
                        trailingText: qsTr("2 min ago")
                    }
                    OsListRow {
                        width: parent.width
                        text: qsTr("Notifications")
                        iconName: "bell"

                        OsBadge {
                            count: 4
                        }
                    }
                    OsListRow {
                        width: parent.width
                        text: qsTr("Selected row")
                        subtitle: qsTr("With a tag in the trailing slot")
                        iconName: "folder"
                        selected: true

                        OsTag {
                            text: qsTr("prod")
                            variant: "accent"
                        }
                    }
                    OsListRow {
                        width: parent.width
                        text: qsTr("Highlighted (current) row")
                        iconName: "file"
                        highlighted: true
                    }
                    OsListRow {
                        width: parent.width
                        text: qsTr("Disabled row")
                        subtitle: qsTr("Not available")
                        iconName: "lock"
                        trailingText: qsTr("Locked")
                        enabled: false
                    }
                }
            }

            // Cards.
            Demo {
                width: section.blockWidth
                title: qsTr("Cards")
                caption: qsTr("OsCard: static, clickable, selected, disabled")

                Grid {
                    columns: 2
                    spacing: Theme.spacingMd

                    Repeater {
                        model: [
                            { title: qsTr("Static card"), body: qsTr("Surface, 1 px border"), clickable: false, selected: false, enabled: true },
                            { title: qsTr("Clickable card"), body: qsTr("Hover, press, focus"), clickable: true, selected: false, enabled: true },
                            { title: qsTr("Selected card"), body: qsTr("Accent outline"), clickable: true, selected: true, enabled: true },
                            { title: qsTr("Disabled card"), body: qsTr("No interaction"), clickable: true, selected: false, enabled: false }
                        ]

                        OsCard {
                            id: card

                            required property var modelData

                            width: (section.blockWidth - Theme.spacingMd) / 2
                            clickable: modelData.clickable
                            selected: modelData.selected
                            enabled: modelData.enabled
                            Accessible.name: modelData.title

                            Column {
                                width: parent.width
                                spacing: Theme.spacingXs

                                OsText {
                                    width: parent.width
                                    text: card.modelData.title
                                    font.weight: Font.DemiBold
                                }
                                OsText {
                                    width: parent.width
                                    text: card.modelData.body
                                    muted: true
                                    size: "small"
                                }
                            }
                        }
                    }
                }
            }

            // Tags.
            Demo {
                width: section.blockWidth
                title: qsTr("Tags")
                caption: qsTr("OsTag: neutral, accent, icon, removable, disabled")

                Flow {
                    width: parent.width
                    spacing: Theme.spacingSm

                    OsTag {
                        text: qsTr("staging")
                    }
                    OsTag {
                        text: qsTr("production")
                        variant: "accent"
                    }
                    OsTag {
                        text: qsTr("linux")
                        iconName: "terminal"
                    }
                    OsTag {
                        text: qsTr("removable")
                        removable: true
                    }
                    OsTag {
                        text: qsTr("accent removable")
                        iconName: "tag"
                        variant: "accent"
                        removable: true
                    }
                    OsTag {
                        text: qsTr("disabled")
                        removable: true
                        enabled: false
                    }
                }
            }

            // Badges.
            Demo {
                width: section.blockWidth
                title: qsTr("Badges")
                caption: qsTr("OsBadge: counts, 99+ and dots in accent, info, success, warning, danger")

                Row {
                    spacing: Theme.spacingMd

                    Repeater {
                        model: ["accent", "info", "success", "warning", "danger"]

                        Row {
                            required property string modelData
                            required property int index

                            spacing: Theme.spacingXs

                            OsBadge {
                                anchors.verticalCenter: parent.verticalCenter
                                variant: parent.modelData
                                count: parent.index === 0 ? 120 : parent.index * 3
                            }
                            OsBadge {
                                anchors.verticalCenter: parent.verticalCenter
                                variant: parent.modelData
                                dot: true
                            }
                        }
                    }
                }
            }

            // Section headers.
            Demo {
                width: section.blockWidth
                title: qsTr("Section headers")
                caption: qsTr("OsSectionHeader: title, description, trailing action")

                // Framed, so the samples don't read as headings of this page.
                Rectangle {
                    width: parent.width
                    height: headerSamples.implicitHeight + 2 * Theme.spacingLg
                    color: Theme.surface
                    radius: Theme.radiusCard
                    border.width: Theme.borderWidth
                    border.color: Theme.border

                    Column {
                        id: headerSamples

                        x: Theme.spacingLg
                        y: Theme.spacingLg
                        width: parent.width - 2 * Theme.spacingLg
                        spacing: Theme.spacingLg

                        OsSectionHeader {
                            width: parent.width
                            title: qsTr("Appearance")
                        }
                        OsSectionHeader {
                            width: parent.width
                            title: qsTr("Terminal")
                            description: qsTr("Font, cursor and scrollback of new terminal sessions.")

                            OsButton {
                                text: qsTr("Reset")
                                variant: "ghost"
                                iconName: "rotate-ccw"
                            }
                        }
                    }
                }
            }

            // Form rows.
            Demo {
                width: section.blockWidth
                title: qsTr("Form rows")
                caption: qsTr("OsFormRow: side by side, help, error, stacked when narrow")

                OsFormRow {
                    width: parent.width
                    label: qsTr("Theme")
                    helpText: qsTr("Follows the system unless you pick one.")

                    OsComboBox {
                        width: parent.width
                        model: [qsTr("Follow the system"), qsTr("Dark"), qsTr("Light")]
                        Accessible.name: qsTr("Theme")
                    }
                }
                OsFormRow {
                    width: parent.width
                    label: qsTr("Scrollback lines")
                    errorText: qsTr("Enter a number between 100 and 100000.")

                    OsTextField {
                        width: parent.width
                        text: qsTr("1000000")
                        error: true
                        Accessible.name: qsTr("Scrollback lines")
                    }
                }
                OsFormRow {
                    width: Math.round(section.blockWidth * 0.55)
                    label: qsTr("Narrow row label")
                    helpText: qsTr("Stacked because the row is narrow.")

                    OsTextField {
                        width: parent.width
                        placeholderText: qsTr("Value")
                        Accessible.name: qsTr("Narrow row label")
                    }
                }
            }

            // Empty state.
            Demo {
                width: section.blockWidth
                title: qsTr("Empty state")
                caption: qsTr("OsEmptyState: icon, title, body, actions")

                Rectangle {
                    width: parent.width
                    height: emptyState.implicitHeight
                    color: Theme.surface
                    radius: Theme.radiusCard
                    border.width: Theme.borderWidth
                    border.color: Theme.border

                    OsEmptyState {
                        id: emptyState

                        anchors.fill: parent
                        iconName: "server"
                        title: qsTr("No hosts yet")
                        description: qsTr("Add a host to connect over SSH, SFTP, RDP or VNC, or import them from another client.")

                        OsButton {
                            text: qsTr("Add host")
                            variant: "primary"
                            iconName: "plus"
                        }
                        OsButton {
                            text: qsTr("Import")
                            iconName: "import"
                        }
                    }
                }
            }

            // Splitter.
            Demo {
                width: section.blockWidth
                title: qsTr("Splitter")
                caption: qsTr("OsSplitter: horizontal with grip, vertical")

                Item {
                    width: parent.width
                    height: Theme.spacingXxl * 4

                    OsSplitter {
                        anchors.fill: parent
                        showGrip: true

                        Rectangle {
                            T.SplitView.preferredWidth: Theme.spacingXxl * 5
                            T.SplitView.minimumWidth: Theme.spacingXxl * 2
                            color: Theme.surface2

                            Caption {
                                anchors.centerIn: parent
                                text: qsTr("Sidebar")
                            }
                        }
                        Rectangle {
                            T.SplitView.fillWidth: true
                            color: Theme.surface

                            OsSplitter {
                                anchors.fill: parent
                                orientation: Qt.Vertical

                                Rectangle {
                                    T.SplitView.fillHeight: true
                                    color: Theme.surface

                                    Caption {
                                        anchors.centerIn: parent
                                        text: qsTr("Terminal")
                                    }
                                }
                                Rectangle {
                                    T.SplitView.preferredHeight: Theme.spacingXxl * 1.5
                                    color: Theme.surface2

                                    Caption {
                                        anchors.centerIn: parent
                                        text: qsTr("Panel")
                                    }
                                }
                            }
                        }
                    }

                    // Frame over the panes.
                    Rectangle {
                        anchors.fill: parent
                        color: "transparent"
                        border.width: Theme.borderWidth
                        border.color: Theme.border
                    }
                }
            }

            // Progress.
            Demo {
                width: section.blockWidth
                title: qsTr("Progress")
                caption: qsTr("OsProgress: 0 %, 40 %, 100 %, indeterminate, disabled")

                Repeater {
                    model: [
                        { label: qsTr("0 %"), value: 0, indeterminate: false, enabled: true, name: qsTr("Upload") },
                        { label: qsTr("40 %"), value: 0.4, indeterminate: false, enabled: true, name: qsTr("Upload") },
                        { label: qsTr("100 %"), value: 1, indeterminate: false, enabled: true, name: qsTr("Upload") },
                        { label: qsTr("Indeterminate"), value: 0, indeterminate: true, enabled: true, name: qsTr("Connecting") },
                        { label: qsTr("Disabled"), value: 0.7, indeterminate: false, enabled: false, name: qsTr("Paused") }
                    ]

                    Row {
                        id: progressRow

                        required property var modelData

                        width: section.blockWidth
                        height: Theme.controlHeightSmall
                        spacing: Theme.spacingMd

                        Caption {
                            id: progressLabel

                            anchors.verticalCenter: parent.verticalCenter
                            width: Theme.spacingXxl * 3
                            text: progressRow.modelData.label
                        }

                        OsProgress {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - progressLabel.width - parent.spacing
                            value: progressRow.modelData.value
                            indeterminate: progressRow.modelData.indeterminate
                            enabled: progressRow.modelData.enabled
                            Accessible.name: progressRow.modelData.name
                        }
                    }
                }
            }
        }
    }
}
