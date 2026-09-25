// Form row: a fixed-width label column, the control (children go into the control slot, e.g.
// `OsTextField { width: parent.width }`), then optional help (muted) and error (danger) texts.
// Below `stackWidth` the label moves above the control. The label is not linked to the control
// for screen readers: give the control an Accessible.name (usually the same text).
//   label: string
//   labelWidth: real      width of the label column (default 160 px at scale 1)
//   helpText: string      optional muted hint under the control
//   errorText: string     optional error under the control; shown with an alert icon
//   stackWidth: real      below this width the row stacks vertically
//   stacked: bool         read-only, true when stacked
import QtQuick
import cc.caixa.opensesh

Item {
    id: control

    property string label
    property real labelWidth: Theme.spacingXxl * 5
    property string helpText
    property string errorText
    property real stackWidth: labelWidth * 2 + Theme.spacingLg
    readonly property bool stacked: width < stackWidth

    default property alias content: slot.data

    readonly property bool mirrored: LayoutMirroring.enabled
    readonly property real gap: Theme.spacingLg
    readonly property real fieldX: stacked ? 0 : labelWidth + gap

    implicitWidth: labelWidth + gap + Math.max(slot.implicitWidth, Theme.spacingXxl * 6)
    implicitHeight: stacked ? labelText.height + Theme.spacingXs + field.implicitHeight
                            : Math.max(labelText.y + labelText.height, field.implicitHeight)

    OsText {
        id: labelText

        x: control.mirrored ? control.width - width : 0
        // Centered on the first control line when side by side.
        y: control.stacked ? 0 : Math.max(0, (Math.min(slot.height > 0 ? slot.height : Theme.controlHeight,
                                                       Theme.controlHeight) - implicitHeight) / 2)
        width: control.stacked ? control.width : control.labelWidth
        text: control.label
        font.weight: Font.Medium
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        horizontalAlignment: Text.AlignLeft
    }

    Column {
        id: field

        x: control.mirrored ? 0 : control.fieldX
        y: control.stacked ? labelText.height + Theme.spacingXs : 0
        width: Math.max(0, control.width - control.fieldX)
        spacing: Theme.spacingXs

        Item {
            id: slot

            width: parent.width
            implicitWidth: {
                let result = 0;
                for (let i = 0; i < children.length; ++i)
                    result = Math.max(result, children[i].implicitWidth);
                return result;
            }
            height: {
                let result = 0;
                for (let i = 0; i < children.length; ++i) {
                    if (children[i].visible)
                        result = Math.max(result, children[i].y + children[i].height);
                }
                return result;
            }
        }

        OsText {
            width: parent.width
            visible: control.helpText.length > 0
            text: control.helpText
            size: "small"
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        Row {
            width: parent.width
            visible: control.errorText.length > 0
            spacing: Theme.spacingXs

            Accessible.role: Accessible.AlertMessage
            Accessible.name: control.errorText

            OsIcon {
                id: errorIcon

                y: Math.max(0, (errorLabel.lineHeightPx - height) / 2)
                name: "circle-alert"
                size: Theme.iconSizeSmall
                color: Theme.danger
            }

            OsText {
                id: errorLabel

                readonly property real lineHeightPx: lineCount > 0 ? implicitHeight / lineCount : implicitHeight

                width: parent.width - errorIcon.width - parent.spacing
                text: control.errorText
                size: "small"
                color: Theme.danger
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
                Accessible.ignored: true
            }
        }
    }
}
