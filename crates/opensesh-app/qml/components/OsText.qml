// Themed text label.
//   muted: bool   secondary text color
//   size: string  "small" | "normal" | "large" | "title"
import QtQuick
import cc.caixa.opensesh

Text {
    id: control

    property bool muted: false
    property string size: "normal"

    color: !enabled ? Theme.textDisabled : muted ? Theme.textMuted : Theme.text
    font.family: Theme.fontFamily
    font.pixelSize: size === "small" ? Theme.fontSizeSmall
                  : size === "large" ? Theme.fontSizeLarge
                  : size === "title" ? Theme.fontSizeTitle
                  : Theme.fontSize
    font.weight: size === "title" || size === "large" ? Font.DemiBold : Font.Normal
    textFormat: Text.PlainText
    elide: Text.ElideRight
    verticalAlignment: Text.AlignVCenter

    Accessible.role: Accessible.StaticText
    Accessible.name: text
}
