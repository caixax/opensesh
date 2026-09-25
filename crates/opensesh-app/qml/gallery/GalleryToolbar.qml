// Gallery toolbar: the title plus the gallery's own appearance switches (theme, density, accent,
// reduce motion). They write the ThemeBinder overrides, never AppSettings, so trying things in
// the gallery doesn't change the user's configuration.
//   binder: ThemeBinder   required; the gallery's binder, with `overrideActive` set
// Functions: setMode(mode), setDensity(density), setAccent(accent), setReduceMotion(on),
// openAccentDialog(), closeAccentDialog() (the custom accent dialog with an OsColorPicker).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Rectangle {
    id: toolbar

    required property ThemeBinder binder

    // Accent swatches: "default" (the §5.2 sesame amber of the current mode, drawn in
    // Theme.defaultAccent), then Theme.accentPresets. The names are index for index with them.
    readonly property var presetNames: [qsTr("Amber"), qsTr("Terracotta"), qsTr("Rose"), qsTr("Lavender"),
        qsTr("Blue"), qsTr("Teal"), qsTr("Green")]
    readonly property var accents: {
        const list = [{ value: "default", name: qsTr("Sesame (default)") }];
        const presets = Theme.accentPresets;
        for (let i = 0; i < presets.length; ++i)
            list.push({ value: presets[i], name: i < presetNames.length ? presetNames[i] : presets[i] });
        return list;
    }
    readonly property string currentAccent: binder.overrideAccent.length > 0 ? binder.overrideAccent
                                                                             : AppSettings.accent
    // Preset matching the current accent ("#rrggbb" in any case), or -1 for a custom color.
    readonly property int accentIndex: {
        const hex = /^#[0-9A-Fa-f]{6}$/.test(currentAccent);
        for (let i = 0; i < accents.length; ++i) {
            if (accents[i].value === currentAccent
                    || (hex && accents[i].value !== "default" && Qt.colorEqual(accents[i].value, currentAccent)))
                return i;
        }
        return -1;
    }

    function setMode(mode: string) {
        binder.overrideMode = mode;
    }

    function setDensity(density: string) {
        binder.overrideDensity = density;
    }

    function setAccent(accent: string) {
        binder.overrideAccent = accent;
    }

    function setReduceMotion(on: bool) {
        binder.overrideReduceMotion = on;
    }

    function openAccentDialog() {
        customAccentDialog.open();
    }

    function closeAccentDialog() {
        customAccentDialog.close();
    }

    implicitHeight: flow.implicitHeight + 2 * Theme.spacingSm + Theme.borderWidth
    color: Theme.surface

    // Two or more mutually exclusive options as a row of buttons; the chosen one is filled.
    component Segmented: Rectangle {
        id: segmented

        property var options: [] // [{ value, text, iconName }]
        property string value
        property string accessibleName

        readonly property real inset: Math.round((Theme.controlHeight - Theme.controlHeightSmall) / 2)

        signal picked(string value)

        implicitWidth: segmentRow.implicitWidth + 2 * inset
        implicitHeight: Theme.controlHeight
        radius: Theme.radiusControl
        color: Theme.surface2
        border.width: Theme.borderWidth
        border.color: Theme.border

        Accessible.role: Accessible.Grouping
        Accessible.name: accessibleName

        Row {
            id: segmentRow

            x: segmented.inset
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacingXs

            Repeater {
                id: segments

                model: segmented.options

                OsButton {
                    id: segment

                    required property var modelData
                    required property int index

                    readonly property bool chosen: segmented.value === modelData.value

                    function pickAt(target: int) {
                        const item = segments.itemAt((target + segments.count) % segments.count);
                        if (!item)
                            return;
                        item.forceActiveFocus(Qt.TabFocusReason);
                        item.clicked();
                    }

                    height: Theme.controlHeightSmall
                    text: modelData.text
                    iconName: modelData.iconName
                    variant: chosen ? "primary" : "ghost"
                    // One Tab stop per group (the chosen option); the arrow keys move the choice.
                    focusPolicy: chosen ? Qt.StrongFocus : Qt.ClickFocus

                    Accessible.role: Accessible.RadioButton
                    Accessible.checkable: true
                    Accessible.checked: chosen

                    onClicked: segmented.picked(modelData.value)
                    Keys.onLeftPressed: pickAt(index - 1)
                    Keys.onRightPressed: pickAt(index + 1)
                }
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: Theme.borderWidth
        color: Theme.border
    }

    Flow {
        id: flow

        // Pushes the switches to the right edge when everything fits on one line.
        readonly property real controlsWidth: themeGroup.width + densityGroup.width + swatches.width
                                              + motionSwitch.width + 3 * spacing

        x: Theme.spacingLg
        y: Theme.spacingSm
        width: parent.width - 2 * Theme.spacingLg
        spacing: Theme.spacingLg

        Row {
            id: titleRow

            height: Theme.controlHeight
            spacing: Theme.spacingSm

            OsIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: "layout-grid"
                color: Theme.accentFg
                size: Theme.iconSize
            }

            OsText {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Component gallery")
                size: "large"
                Accessible.role: Accessible.Heading
            }
        }

        Item {
            width: Math.max(0, flow.width - titleRow.width - flow.controlsWidth - 2 * flow.spacing)
            height: Theme.controlHeight
            visible: width > 0
        }

        Segmented {
            id: themeGroup

            accessibleName: qsTr("Theme")
            value: toolbar.binder.overrideMode
            options: [
                { value: "dark", text: qsTr("Dark"), iconName: "moon" },
                { value: "light", text: qsTr("Light"), iconName: "sun" }
            ]
            onPicked: value => toolbar.setMode(value)
        }

        Segmented {
            id: densityGroup

            accessibleName: qsTr("Density")
            value: toolbar.binder.overrideDensity
            options: [
                { value: "comfortable", text: qsTr("Comfortable"), iconName: "rows-2" },
                { value: "compact", text: qsTr("Compact"), iconName: "list" }
            ]
            onPicked: value => toolbar.setDensity(value)
        }

        Row {
            id: swatches

            height: Theme.controlHeight
            spacing: Theme.spacingXs

            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("Accent")

            Repeater {
                id: swatchRepeater

                model: toolbar.accents

                // Same look and keys as the OsColorPicker swatches: one Tab stop, arrows move.
                delegate: T.AbstractButton {
                    id: swatch

                    required property var modelData
                    required property int index

                    readonly property bool selected: toolbar.accentIndex === index
                    readonly property color swatchColor: modelData.value !== "default" ? modelData.value
                                                                                       : Theme.defaultAccent

                    function moveTo(target: int) {
                        const count = swatchRepeater.count;
                        const item = swatchRepeater.itemAt((target + count) % count);
                        if (item)
                            item.forceActiveFocus(Qt.TabFocusReason);
                    }

                    anchors.verticalCenter: parent.verticalCenter
                    implicitWidth: Theme.controlHeightSmall
                    implicitHeight: Theme.controlHeightSmall
                    padding: 0
                    hoverEnabled: true
                    focusPolicy: index === Math.max(0, toolbar.accentIndex) ? Qt.StrongFocus : Qt.ClickFocus

                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.name
                    Accessible.checkable: true
                    Accessible.checked: selected

                    onClicked: toolbar.setAccent(modelData.value)

                    Keys.onLeftPressed: moveTo(index - 1)
                    Keys.onRightPressed: moveTo(index + 1)
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Home)
                            moveTo(0);
                        else if (event.key === Qt.Key_End)
                            moveTo(swatchRepeater.count - 1);
                        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            toolbar.setAccent(modelData.value);
                        else
                            return;
                        event.accepted = true;
                    }

                    contentItem: Item {
                        OsIcon {
                            anchors.centerIn: parent
                            visible: swatch.selected
                            name: "check"
                            size: Theme.iconSizeSmall
                            color: Theme.textOn(chip.color)
                        }
                    }

                    background: Rectangle {
                        id: chip

                        radius: width / 2
                        color: swatch.swatchColor
                        border.width: Theme.borderWidth
                        border.color: swatch.hovered || swatch.down ? Theme.text : Theme.border

                        Behavior on border.color {
                            ColorAnimation {
                                duration: Theme.durationFast
                            }
                        }

                        OsFocusRing {
                            target: swatch
                            baseRadius: chip.radius
                        }
                    }

                    OsTooltip {
                        visible: swatch.hovered
                        text: swatch.modelData.name
                    }
                }
            }

            OsIconButton {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "palette"
                toolTip: qsTr("Custom accent")
                checkable: true
                checked: toolbar.accentIndex < 0
                onClicked: {
                    // Keep the check state tied to the current accent, not to the click.
                    checked = Qt.binding(() => toolbar.accentIndex < 0);
                    customAccentDialog.open();
                }
            }
        }

        OsSwitch {
            id: motionSwitch

            height: Theme.controlHeight
            text: qsTr("Reduce motion")
            checked: toolbar.binder.overrideReduceMotion === true
            onToggled: toolbar.setReduceMotion(checked)
        }
    }

    OsDialog {
        id: customAccentDialog

        title: qsTr("Custom accent")
        acceptText: qsTr("Done")
        showReject: false

        Column {
            width: Theme.spacingXs * 88
            spacing: Theme.spacingMd

            OsText {
                width: parent.width
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: qsTr("Pick a swatch or type a hex code: the gallery previews it at once. The user's accent setting is not changed.")
            }

            OsColorPicker {
                id: accentPicker

                showDefault: true
                defaultSelected: toolbar.currentAccent === "default"
                onAccepted: value => toolbar.setAccent(accentPicker.hexOf(value))
                onDefaultPicked: toolbar.setAccent("default")
            }

            Row {
                spacing: Theme.spacingSm
                visible: Theme.accentLowContrast

                OsIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "triangle-alert"
                    color: Theme.warning
                    size: Theme.iconSizeSmall
                }

                OsText {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Low contrast: this accent is under 3:1 against the background.")
                    muted: true
                }
            }
        }
    }

    // A pick sets the picker's value itself, which would break a plain binding: this keeps it in
    // step with the accent the toolbar swatches choose.
    Binding {
        target: accentPicker
        property: "value"
        value: Theme.accent
    }
}
