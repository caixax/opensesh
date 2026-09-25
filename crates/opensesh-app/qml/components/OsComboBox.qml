// Drop-down list (see docs/design/components.md). Uses the Qt ComboBox API: `model` (an array
// of strings, an array of objects with `textRole`/`valueRole`, or any item model),
// `currentIndex`, `currentText`, `currentValue`, `displayText`, `activated(index)`.
// Keyboard: Up/Down/Home/End change the selection, Space opens the list, typing a letter jumps
// to the next item that starts with it, Enter picks and Escape closes.
//   previewFontFamilies: bool  draws every item in the font family named by its text
//                              (used by OsFontPicker)
//   maxVisibleItems: int       rows shown before the list scrolls (default 10)
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.ComboBox {
    id: control

    property bool previewFontFamilies: false
    property int maxVisibleItems: 10

    readonly property color inkColor: enabled ? Theme.text : Theme.textDisabled
    readonly property real indicatorSpace: Theme.spacingSm + (indicator ? indicator.width + spacing : 0)

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    leftPadding: mirrored ? indicatorSpace : Theme.controlPadding
    rightPadding: mirrored ? Theme.controlPadding : indicatorSpace
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacingSm
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    delegate: T.ItemDelegate {
        id: item

        required property int index

        readonly property bool current: control.currentIndex === index

        width: ListView.view ? ListView.view.width : implicitWidth
        implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                                implicitContentWidth + leftPadding + rightPadding)
        implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                                 implicitContentHeight + topPadding + bottomPadding)
        leftPadding: Theme.controlPadding - Theme.spacingXs
        rightPadding: Theme.spacingSm
        topPadding: 0
        bottomPadding: 0
        spacing: Theme.spacingSm
        text: control.textAt(index)
        highlighted: control.highlightedIndex === index
        hoverEnabled: control.hoverEnabled
        font.family: control.previewFontFamilies && text.length > 0 ? text : control.font.family
        font.pixelSize: control.font.pixelSize
        font.weight: current ? Font.DemiBold : Font.Normal

        Accessible.role: Accessible.ListItem
        Accessible.name: text
        Accessible.selected: current

        contentItem: Item {
            implicitWidth: label.implicitWidth + item.spacing + check.width
            implicitHeight: label.implicitHeight

            OsText {
                id: label

                anchors.left: parent.left
                anchors.right: check.left
                anchors.rightMargin: item.spacing
                anchors.verticalCenter: parent.verticalCenter
                text: item.text
                font: item.font
                color: item.current ? Theme.accentFg : Theme.text
            }

            OsIcon {
                id: check

                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                name: "check"
                size: Theme.iconSizeSmall
                color: Theme.accentFg
                opacity: item.current ? 1 : 0
            }
        }

        background: Rectangle {
            implicitHeight: Theme.controlHeight
            radius: Theme.radiusSmall
            color: item.down ? Theme.pressed : item.highlighted ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }
    }

    indicator: OsIcon {
        x: control.mirrored ? Theme.spacingSm : control.width - width - Theme.spacingSm
        y: Math.round((control.height - height) / 2)
        name: "chevron-down"
        size: Theme.iconSizeSmall
        color: control.enabled ? Theme.textMuted : Theme.textDisabled
        rotation: control.popup.visible ? 180 : 0

        Behavior on rotation {
            NumberAnimation {
                duration: Theme.durationFast
            }
        }
    }

    contentItem: OsText {
        text: control.displayText
        font: control.font
        color: control.inkColor
        horizontalAlignment: Text.AlignLeft
    }

    background: Rectangle {
        implicitWidth: Theme.controlHeight * 5
        implicitHeight: Theme.controlHeight
        radius: Theme.radiusControl
        color: Theme.surface2
        border.width: control.enabled && control.popup.visible ? Theme.focusRingWidth : Theme.borderWidth
        border.color: !control.enabled ? Theme.border
                                       : control.popup.visible ? Theme.accent : Theme.borderStrong

        Behavior on border.color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the fill.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled
            color: control.pressed ? Theme.pressed : control.hovered ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusControl
        }
    }

    popup: T.Popup {
        y: control.height + Theme.spacingXs
        width: control.width
        height: Math.min(implicitHeight, control.Window.height - topMargin - bottomMargin)
        implicitHeight: Math.min(contentItem.implicitHeight, Theme.controlHeight * control.maxVisibleItems)
                        + topPadding + bottomPadding
        topMargin: Theme.spacingSm
        bottomMargin: Theme.spacingSm
        padding: Theme.spacingXs

        enter: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: Theme.durationFast
            }
        }
        exit: Transition {
            NumberAnimation {
                property: "opacity"
                from: 1
                to: 0
                duration: Theme.durationFast
            }
        }

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.delegateModel
            currentIndex: control.highlightedIndex
            highlightMoveDuration: 0
            boundsBehavior: Flickable.StopAtBounds

            T.ScrollBar.vertical: OsScrollBar {}
        }

        background: Rectangle {
            color: Theme.surface
            radius: Theme.radiusControl
            border.width: Theme.borderWidth
            border.color: Theme.borderStrong
        }
    }
}
