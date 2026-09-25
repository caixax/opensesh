// Gallery section: the type scale (OsText sizes in Theme.fontFamily), the monospace font, the
// spacing scale, radii and strokes, control and shell sizes, icons (OsIcon) and motion
// durations, all read live from Theme. Give it a width; the height is implicit.
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Column {
    id: section

    // Width of the name column in the token tables.
    readonly property real nameWidth: Theme.spacingXxl * 6
    // Every UI icon in assets/icons/icons.toml ([icons], Lucide set).
    readonly property var iconNames: ["app-window", "arrow-left-right", "bell", "book-open", "bug",
        "check", "chevron-down", "chevron-left", "chevron-right", "chevron-up", "circle-alert",
        "circle-check", "circle-help", "circle-x", "columns-2", "command", "copy", "cpu",
        "door-open", "download", "ellipsis", "external-link", "eye", "eye-off", "file", "file-text",
        "folder", "folder-open", "folder-sync", "globe", "grip-horizontal", "grip-vertical",
        "hard-drive", "history", "house", "import", "info", "key-round", "keyboard", "languages",
        "layout-grid", "link", "list", "list-tree", "lock", "lock-open", "log-out", "maximize-2",
        "memory-stick", "minimize-2", "minus", "monitor", "moon", "network", "palette",
        "panel-bottom", "panel-left", "panel-right", "panel-right-close", "panel-right-open",
        "pencil", "plug-zap", "plus", "radio-tower", "refresh-cw", "rotate-ccw", "rows-2",
        "scroll-text", "search", "server", "settings", "shield-check", "sliders-horizontal",
        "square", "square-terminal", "star", "sun", "sun-moon", "tag", "terminal", "trash-2",
        "triangle-alert", "type", "unplug", "upload", "usb", "user", "waypoints", "x", "zap"]
    // Host OS logos (Simple Icons and Tabler).
    readonly property var osIconNames: ["os-almalinux", "os-alpinelinux", "os-apple", "os-archlinux",
        "os-debian", "os-docker", "os-fedora", "os-freebsd", "os-gentoo", "os-kubernetes", "os-linux",
        "os-linuxmint", "os-manjaro", "os-nixos", "os-opensuse", "os-podman", "os-raspberrypi",
        "os-redhat", "os-rockylinux", "os-ubuntu", "os-windows"]

    function px(value: real): string {
        return qsTr("%1 px").arg(Math.round(value * 10) / 10);
    }

    function ms(value: real): string {
        return qsTr("%1 ms").arg(Math.round(value));
    }

    spacing: Theme.spacingXl

    component Group: Column {
        id: group

        property string title
        property string description
        default property alias content: body.data

        width: parent ? parent.width : implicitWidth
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: group.title
            description: group.description
        }

        Column {
            id: body

            width: parent.width
            spacing: Theme.spacingMd
        }
    }

    // Token name (monospace) and its value, as the leading column of a table row.
    component TokenLabel: Column {
        property string name
        property string value

        width: section.nameWidth
        spacing: 0

        OsText {
            width: parent.width
            text: parent.name
            size: "small"
            font.family: Theme.monoFontFamily
        }

        OsText {
            width: parent.width
            text: parent.value
            size: "small"
            muted: true
        }
    }

    Column {
        width: parent.width
        spacing: Theme.spacingSm

        OsText {
            text: qsTr("Typography and spacing")
            size: "title"
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            text: qsTr("Sizes, spacing, shapes, icons and motion. Everything follows the UI scale; control and shell sizes also follow the density.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    Group {
        title: qsTr("Type scale")
        description: qsTr("OsText sizes in the UI font, %1. Titles and large text are DemiBold, buttons Medium, body text Normal.")
            .arg(Theme.fontFamily)

        Repeater {
            model: [
                { size: "title", token: "fontSizeTitle", value: Theme.fontSizeTitle, weight: qsTr("DemiBold") },
                { size: "large", token: "fontSizeLarge", value: Theme.fontSizeLarge, weight: qsTr("DemiBold") },
                { size: "normal", token: "fontSize", value: Theme.fontSize, weight: qsTr("Normal") },
                { size: "small", token: "fontSizeSmall", value: Theme.fontSizeSmall, weight: qsTr("Normal") }
            ]

            Row {
                id: typeRow

                required property var modelData

                width: parent.width
                spacing: Theme.spacingLg

                TokenLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    name: typeRow.modelData.token
                    value: qsTr("%1, %2").arg(section.px(typeRow.modelData.value)).arg(typeRow.modelData.weight)
                }

                OsText {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - section.nameWidth - parent.spacing
                    size: typeRow.modelData.size
                    text: qsTr("Open sesame: hosts, terminals and files in one calm window")
                }
            }
        }

        Row {
            width: parent.width
            spacing: Theme.spacingLg

            TokenLabel {
                anchors.verticalCenter: parent.verticalCenter
                name: qsTr("Weights")
                value: qsTr("Normal, Medium, DemiBold")
            }

            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spacingXl

                OsText {
                    text: qsTr("Body text")
                }
                OsText {
                    text: qsTr("Button label")
                    font.weight: Font.Medium
                }
                OsText {
                    text: qsTr("Heading")
                    font.weight: Font.DemiBold
                }
                OsText {
                    text: qsTr("Muted caption")
                    muted: true
                }
                OsText {
                    text: qsTr("Disabled text")
                    enabled: false
                }
                OsText {
                    text: qsTr("Accent link")
                    color: Theme.accentFg
                }
            }
        }
    }

    Group {
        title: qsTr("Monospace")
        description: qsTr("Terminal and code font, %1 (Theme.monoFontFamily).").arg(Theme.monoFontFamily)

        Rectangle {
            width: parent.width
            height: monoColumn.implicitHeight + 2 * Theme.spacingLg
            radius: Theme.radiusCard
            color: Theme.surface
            border.width: Theme.borderWidth
            border.color: Theme.border

            Column {
                id: monoColumn

                x: Theme.spacingLg
                y: Theme.spacingLg
                width: parent.width - 2 * Theme.spacingLg
                spacing: Theme.spacingXs

                Repeater {
                    model: [
                        qsTr("deploy@web-01:~$ ssh -J bastion admin@10.0.0.12 -p 2222"),
                        qsTr("0O 1lI| {} [] () <= >= != -> => ~/ .. :: 0x1F"),
                        qsTr("The quick brown fox jumps over the lazy dog 1234567890")
                    ]

                    OsText {
                        required property string modelData
                        required property int index

                        width: parent.width
                        text: modelData
                        font.family: Theme.monoFontFamily
                        // Programming ligatures are off by default in the terminal (PLAN §6.2).
                        font.features: { "calt": 0, "liga": 0 }
                        color: index === 0 ? Theme.accentFg : Theme.text
                    }
                }
            }
        }
    }

    Group {
        title: qsTr("Spacing scale")
        description: qsTr("4 px steps for gaps, margins and paddings. The squares have the real size; density does not change them.")

        Flow {
            width: parent.width
            spacing: Theme.spacingXl

            Repeater {
                model: [
                    { token: "spacingXs", value: Theme.spacingXs },
                    { token: "spacingSm", value: Theme.spacingSm },
                    { token: "spacingMd", value: Theme.spacingMd },
                    { token: "spacingLg", value: Theme.spacingLg },
                    { token: "spacingXl", value: Theme.spacingXl },
                    { token: "spacingXxl", value: Theme.spacingXxl }
                ]

                Column {
                    id: spacingCell

                    required property var modelData

                    spacing: Theme.spacingSm

                    // Squares stand on a common baseline.
                    Item {
                        width: Theme.spacingXxl * 3
                        height: Theme.spacingXxl

                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: spacingCell.modelData.value
                            height: spacingCell.modelData.value
                            radius: Theme.borderWidth
                            color: Theme.accent
                        }
                    }

                    TokenLabel {
                        width: Theme.spacingXxl * 3
                        name: spacingCell.modelData.token
                        value: section.px(spacingCell.modelData.value)
                    }
                }
            }
        }
    }

    Group {
        title: qsTr("Radii and strokes")
        description: qsTr("Cards and dialogs use radiusCard, controls radiusControl, small items radiusSmall. Hairlines are borderWidth; the focus ring is focusRingWidth.")

        Flow {
            width: parent.width
            spacing: Theme.spacingXl

            Repeater {
                model: [
                    { token: "radiusSmall", value: Theme.radiusSmall, radius: Theme.radiusSmall, stroke: Theme.borderWidth, ring: false },
                    { token: "radiusControl", value: Theme.radiusControl, radius: Theme.radiusControl, stroke: Theme.borderWidth, ring: false },
                    { token: "radiusCard", value: Theme.radiusCard, radius: Theme.radiusCard, stroke: Theme.borderWidth, ring: false },
                    { token: "borderWidth", value: Theme.borderWidth, radius: Theme.radiusControl, stroke: Theme.borderWidth, ring: false },
                    { token: "focusRingWidth", value: Theme.focusRingWidth, radius: Theme.radiusControl, stroke: Theme.focusRingWidth, ring: true }
                ]

                Column {
                    id: shapeCell

                    required property var modelData

                    spacing: Theme.spacingSm

                    Rectangle {
                        width: Theme.spacingXxl * 4
                        height: Theme.spacingXxl * 2
                        radius: shapeCell.modelData.radius
                        color: Theme.surface2
                        border.width: shapeCell.modelData.stroke
                        border.color: shapeCell.modelData.ring ? Theme.focusRing : Theme.borderStrong
                    }

                    TokenLabel {
                        name: shapeCell.modelData.token
                        value: section.px(shapeCell.modelData.value)
                        width: Theme.spacingXxl * 4
                    }
                }
            }
        }
    }

    Group {
        title: qsTr("Control and shell heights")
        description: qsTr("They follow the density: compare Comfortable and Compact in the toolbar. The bars have the real height.")

        // Bars standing on a common baseline.
        Flow {
            width: parent.width
            spacing: Theme.spacingXl

            Repeater {
                id: heightRepeater

                model: [
                    { token: "controlHeightSmall", value: Theme.controlHeightSmall },
                    { token: "controlHeight", value: Theme.controlHeight },
                    { token: "rowHeight", value: Theme.rowHeight },
                    { token: "statusBarHeight", value: Theme.statusBarHeight },
                    { token: "titleBarHeight", value: Theme.titleBarHeight }
                ]

                Column {
                    id: heightCell

                    required property var modelData

                    spacing: Theme.spacingSm

                    Item {
                        width: Theme.spacingXxl * 4
                        height: Theme.titleBarHeight

                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width
                            height: heightCell.modelData.value
                            radius: Theme.radiusControl
                            color: Theme.surface2
                            border.width: Theme.borderWidth
                            border.color: Theme.borderStrong
                        }
                    }

                    TokenLabel {
                        width: Theme.spacingXxl * 4
                        name: heightCell.modelData.token
                        value: section.px(heightCell.modelData.value)
                    }
                }
            }
        }

    }

    Group {
        title: qsTr("Widths and paddings")
        description: qsTr("Also follow the density. The bars have the real length.")

        Repeater {
            model: [
                { token: "controlPadding", value: Theme.controlPadding },
                { token: "iconSizeSmall", value: Theme.iconSizeSmall },
                { token: "iconSize", value: Theme.iconSize },
                { token: "railWidth", value: Theme.railWidth },
                { token: "railWidthLabels", value: Theme.railWidthLabels }
            ]

            Row {
                id: widthRow

                required property var modelData

                spacing: Theme.spacingLg

                TokenLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    name: widthRow.modelData.token
                    value: section.px(widthRow.modelData.value)
                }

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: widthRow.modelData.value
                    height: Theme.spacingLg
                    radius: Theme.borderWidth
                    color: Theme.accent
                }
            }
        }
    }

    Group {
        title: qsTr("Icons")
        description: qsTr("OsIcon renders the pinned icon sets at iconSizeSmall (buttons, menus) and iconSize (rail, headers), in any token color.")

        Row {
            spacing: Theme.spacingLg

            Repeater {
                model: [
                    { name: "text", color: Theme.text },
                    { name: "textMuted", color: Theme.textMuted },
                    { name: "textDisabled", color: Theme.textDisabled },
                    { name: "accentFg", color: Theme.accentFg },
                    { name: "success", color: Theme.success },
                    { name: "warning", color: Theme.warning },
                    { name: "danger", color: Theme.danger },
                    { name: "info", color: Theme.info }
                ]

                Column {
                    id: colorCell

                    required property var modelData

                    spacing: Theme.spacingXs

                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: Theme.spacingSm

                        OsIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "server"
                            color: colorCell.modelData.color
                            size: Theme.iconSizeSmall
                        }
                        OsIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "server"
                            color: colorCell.modelData.color
                            size: Theme.iconSize
                        }
                    }

                    OsText {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: colorCell.modelData.name
                        size: "small"
                        muted: true
                        font.family: Theme.monoFontFamily
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: iconGrid.implicitHeight + 2 * Theme.spacingLg
            radius: Theme.radiusCard
            color: Theme.surface
            border.width: Theme.borderWidth
            border.color: Theme.border

            Flow {
                id: iconGrid

                readonly property int columns: Math.max(1, Math.floor((width + spacing) / (Theme.spacingXxl * 4.5 + spacing)))
                readonly property real cellWidth: Math.floor((width - (columns - 1) * spacing) / columns)

                x: Theme.spacingLg
                y: Theme.spacingLg
                width: parent.width - 2 * Theme.spacingLg
                spacing: Theme.spacingSm

                Repeater {
                    model: section.iconNames.concat(section.osIconNames)

                    Row {
                        id: iconCell

                        required property string modelData

                        width: iconGrid.cellWidth
                        height: Theme.controlHeightSmall
                        spacing: Theme.spacingSm

                        OsIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: iconCell.modelData
                            size: Theme.iconSize
                        }

                        OsText {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - Theme.iconSize - parent.spacing
                            text: iconCell.modelData
                            size: "small"
                            muted: true
                        }
                    }
                }
            }
        }
    }

    Group {
        title: qsTr("Motion")
        description: qsTr("State changes animate with durationFast, larger movements with durationNormal. Both are 0 with reduce motion.")

        Row {
            spacing: Theme.spacingXl

            TokenLabel {
                anchors.verticalCenter: parent.verticalCenter
                name: "durationFast"
                value: section.ms(Theme.durationFast)
            }

            TokenLabel {
                anchors.verticalCenter: parent.verticalCenter
                name: "durationNormal"
                value: section.ms(Theme.durationNormal)
            }

            OsButton {
                id: playButton

                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Play")
                iconName: "arrow-left-right"
                onClicked: motionTrack.atEnd = !motionTrack.atEnd
            }

            Rectangle {
                id: motionTrack

                property bool atEnd: false

                anchors.verticalCenter: parent.verticalCenter
                width: Theme.spacingXxl * 8
                height: Theme.controlHeightSmall
                radius: height / 2
                color: Theme.surface2
                border.width: Theme.borderWidth
                border.color: Theme.border

                Accessible.role: Accessible.Animation
                Accessible.name: qsTr("Motion sample")

                Rectangle {
                    x: motionTrack.atEnd ? motionTrack.width - width - Theme.spacingXs : Theme.spacingXs
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.height - 2 * Theme.spacingXs
                    height: width
                    radius: width / 2
                    color: Theme.accent

                    Behavior on x {
                        NumberAnimation {
                            duration: Theme.durationNormal
                            easing.type: Easing.OutCubic
                        }
                    }
                }
            }
        }
    }
}
