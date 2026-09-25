// Card: a `surface` panel with a 1 px border and `radiusCard` corners. Children go into the
// padded content area and size the card implicitly (the largest child's implicit size); give
// them `width: parent.width` to fill it.
//   hoverable: bool       hover overlay
//   clickable: bool       hover and press overlays, pointer cursor, Tab stop; Space/Enter click;
//                         clicks on child controls stay with them
//   selected: bool        accent outline
//   content: list<Item>   default property
//   signal clicked
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

// T.Control rather than T.Pane: a Pane accepts every mouse press itself, so a click area behind
// the content would never see one.
T.Control {
    id: control

    property bool hoverable: false
    property bool clickable: false
    property bool selected: false

    default property alias content: body.data

    signal clicked

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingLg
    hoverEnabled: hoverable || clickable
    focusPolicy: clickable ? Qt.StrongFocus : Qt.NoFocus

    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize

    Accessible.role: clickable ? Accessible.Button : Accessible.Pane
    Accessible.selected: selected
    Accessible.onPressAction: {
        if (control.clickable && control.enabled)
            control.clicked();
    }

    // Keyboard activation, only when the card itself (not a child control) has the focus.
    Keys.onPressed: event => {
        if (!control.clickable || control.Window.activeFocusItem !== control)
            return;
        if (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            control.clicked();
            event.accepted = true;
        }
    }

    contentItem: Item {
        id: body

        implicitWidth: {
            let result = 0;
            for (let i = 0; i < children.length; ++i)
                result = Math.max(result, children[i].implicitWidth);
            return result;
        }
        implicitHeight: {
            let result = 0;
            for (let i = 0; i < children.length; ++i)
                result = Math.max(result, children[i].implicitHeight);
            return result;
        }
    }

    background: Rectangle {
        implicitWidth: Theme.spacingXxl * 4
        implicitHeight: Theme.spacingXxl * 2
        radius: Theme.radiusCard
        color: Theme.surface
        border.width: control.selected ? Theme.borderWidth * 2 : Theme.borderWidth
        border.color: control.selected ? Theme.accent : Theme.border

        Behavior on border.color {
            ColorAnimation {
                duration: Theme.durationFast
            }
        }

        // Hover and press feedback, drawn over the fill.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: control.enabled && control.hoverEnabled
            color: clickArea.pressed ? Theme.pressed : control.hovered ? Theme.hover : "transparent"

            Behavior on color {
                ColorAnimation {
                    duration: Theme.durationFast
                }
            }
        }

        // Behind the content, so child controls keep their own clicks.
        MouseArea {
            id: clickArea

            anchors.fill: parent
            enabled: control.clickable && control.enabled
            cursorShape: Qt.PointingHandCursor
            onClicked: control.clicked()
        }

        OsFocusRing {
            target: control
            baseRadius: Theme.radiusCard
        }
    }
}
