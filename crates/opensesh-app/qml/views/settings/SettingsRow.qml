// A settings row: an OsFormRow that fills its column, with a label column wide enough for the
// settings labels, stacking the label above the control when the control would get narrower than
// `minimumFieldWidth`. Children go into the control slot (see OsFormRow).
//   minimumFieldWidth: real   narrowest control column before the row stacks
import QtQuick
import cc.caixa.opensesh

OsFormRow {
    property real minimumFieldWidth: Theme.spacingXxl * 9

    width: parent ? parent.width : implicitWidth
    labelWidth: Theme.spacingXxl * 6
    stackWidth: labelWidth + gap + minimumFieldWidth
}
