// Themed tooltip. Declare it inside the item it describes and bind `visible`:
//   OsButton { id: save; OsTooltip { visible: save.hovered; text: qsTr("Save") } }
// Differences from T.ToolTip: `delay` defaults to 600 ms and `timeout` to 5000 ms; it sits
// centered above its parent and wraps long text at `maxWidth`. T.ToolTip already makes `text`
// the accessible name (role ToolTip).
//   maxWidth: real  wrap width (320 logical px at scale 1)
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.ToolTip {
    id: control

    property real maxWidth: Theme.spacingXs * 80

    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: -height - Theme.spacingXs

    width: Math.min(implicitWidth, maxWidth)

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // Distance kept from the window edges.
    margins: Theme.spacingSm
    leftPadding: Theme.spacingSm
    rightPadding: Theme.spacingSm
    topPadding: Math.round(Theme.spacingXs * 1.5)
    bottomPadding: Math.round(Theme.spacingXs * 1.5)

    delay: 600
    timeout: 5000
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent | T.Popup.CloseOnReleaseOutsideParent

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSizeSmall

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

    contentItem: OsText {
        text: control.text
        font: control.font
        color: Theme.text
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        Accessible.ignored: true
    }

    background: Rectangle {
        color: Theme.surface2
        radius: Theme.radiusSmall
        border.color: Theme.borderStrong
        border.width: Theme.borderWidth
    }
}
