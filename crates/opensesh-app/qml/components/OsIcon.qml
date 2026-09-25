// Monochrome icon from the pinned icon sets (assets/icons/icons.toml), rendered by the
// `image://icon` provider at the physical pixel size.
//   name: string  internal icon name, e.g. "search"
//   color: color  defaults to Theme.text
//   size: real    logical size, defaults to Theme.iconSize
import QtQuick
import cc.caixa.opensesh

Item {
    id: icon

    property string name
    property color color: Theme.text
    property real size: Theme.iconSize

    implicitWidth: size
    implicitHeight: size

    Accessible.ignored: true

    Image {
        anchors.fill: parent
        visible: icon.name.length > 0
        source: visible ? "image://icon/" + icon.name + "?color=" + encodeURIComponent(icon.color.toString())
                          + "&size=" + Math.round(icon.size) : ""
        // With sourceSize set, Qt asks the provider for size x devicePixelRatio pixels.
        sourceSize: Qt.size(Math.round(icon.size), Math.round(icon.size))
        fillMode: Image.PreserveAspectFit
        smooth: true
        cache: true
    }
}
