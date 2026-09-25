// Count or dot badge in a status color; the number uses Theme.textOn(fill). Hidden when `count`
// is 0 unless `dot` is set (a dot never shows the count).
//   count: int         number to show; above maxCount shows "<maxCount>+"
//   dot: bool          small dot without a number
//   variant: string    "accent" (default) | "info" | "success" | "warning" | "danger"
//   maxCount: int      default 99
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: control

    property int count: 0
    property bool dot: false
    property string variant: "accent"
    property int maxCount: 99

    readonly property color fillColor: {
        switch (variant) {
        case "info":
            return Theme.info;
        case "success":
            return Theme.success;
        case "warning":
            return Theme.warning;
        case "danger":
            return Theme.danger;
        default:
            return Theme.accent;
        }
    }
    readonly property string label: count > maxCount ? qsTr("%1+").arg(maxCount) : String(count)

    visible: dot || count > 0
    implicitHeight: dot ? Theme.spacingSm : Math.round(Theme.fontSizeSmall * 1.5)
    implicitWidth: dot ? implicitHeight : Math.max(implicitHeight, countText.implicitWidth + Theme.spacingSm)
    radius: height / 2
    color: fillColor

    Accessible.role: Accessible.StaticText
    Accessible.name: dot ? qsTr("New activity") : label

    OsText {
        id: countText

        anchors.centerIn: parent
        visible: !control.dot
        text: control.label
        size: "small"
        font.weight: Font.DemiBold
        color: Theme.textOn(control.fillColor)
        horizontalAlignment: Text.AlignHCenter
        Accessible.ignored: true
    }
}
