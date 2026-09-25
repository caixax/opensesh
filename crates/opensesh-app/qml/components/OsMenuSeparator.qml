// Hairline between groups of entries in an OsContextMenu (Accessible.role Separator).
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

T.MenuSeparator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    leftPadding: Theme.spacingXs
    rightPadding: Theme.spacingXs
    topPadding: Theme.spacingXs
    bottomPadding: Theme.spacingXs

    Accessible.role: Accessible.Separator

    contentItem: Rectangle {
        implicitWidth: Theme.spacingXs * 10
        implicitHeight: Theme.borderWidth
        color: Theme.border
    }
}
