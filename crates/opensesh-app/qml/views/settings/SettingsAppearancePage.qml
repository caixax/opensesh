// Settings > Appearance (PLAN §6.1): theme, accent, density, UI scale and font, motion, and the
// shell layout (rail, side panel, tabs, status bar, window decorations), with a live preview.
// Every control reads and writes AppSettings live; the UI scale is saved shortly after the slider
// is released (mouse or arrow keys), so dragging it doesn't re-layout the whole UI at every step.
import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property string defaultFontLabel: qsTr("Default (Inter)")

    function decorationName(mode: string): string {
        switch (mode) {
        case "custom":
            return qsTr("the OpenSesh title bar");
        case "native":
            return qsTr("the system title bar");
        case "none":
            return qsTr("no window buttons");
        default:
            return mode;
        }
    }

    // Saves the slider's value as the UI scale (rounded to the slider step).
    function commitScale() {
        const scale = Math.round(scaleSlider.value * 100) / 100;
        if (Math.abs(scale - AppSettings.uiScale) > 0.001)
            AppSettings.uiScale = scale;
    }

    title: qsTr("Appearance")
    description: qsTr("How OpenSesh looks: theme, accent color, density, text size and the layout of the window.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Preview")

        SettingsPreview {
            width: parent.width
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Theme and text")

        SettingsRow {
            label: qsTr("Theme")
            helpText: qsTr("System follows your desktop's light or dark setting (currently %1).")
                      .arg(Theme.systemDark ? qsTr("dark") : qsTr("light"))

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("theme")
                labels: ({
                        system: qsTr("System"),
                        dark: qsTr("Dark"),
                        light: qsTr("Light")
                    })
                icons: ({
                        system: "sun-moon",
                        dark: "moon",
                        light: "sun"
                    })
                value: AppSettings.theme
                Accessible.name: qsTr("Theme")
                onPicked: value => AppSettings.theme = value
            }
        }

        SettingsRow {
            label: qsTr("Accent color")
            helpText: qsTr("Used for buttons, selections and highlights. Text on it is adjusted to stay readable.")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsColorPicker {
                    id: accentPicker

                    // The first swatch is the default accent of the current mode: picking it
                    // stores "default", so the accent keeps following the light or dark mode.
                    showDefault: true
                    defaultSelected: AppSettings.accent === "default"
                    Accessible.name: qsTr("Accent color")
                    onAccepted: picked => AppSettings.accent = accentPicker.hexOf(picked)
                    onDefaultPicked: AppSettings.accent = "default"
                }

                OsButton {
                    // Stays enabled (a no-op on the default accent): disabling it under the
                    // keyboard focus would drop the focus.
                    text: qsTr("Use the default accent")
                    iconName: "rotate-ccw"
                    onClicked: AppSettings.accent = "default"
                }

                Row {
                    width: parent.width
                    visible: Theme.accentLowContrast
                    spacing: Theme.spacingXs

                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: lowContrastText.text

                    OsIcon {
                        id: lowContrastIcon

                        y: Math.max(0, (lowContrastText.lineHeightPx - height) / 2)
                        name: "triangle-alert"
                        size: Theme.iconSizeSmall
                        color: Theme.warning
                    }

                    OsText {
                        id: lowContrastText

                        readonly property real lineHeightPx: lineCount > 0 ? implicitHeight / lineCount : implicitHeight

                        width: parent.width - lowContrastIcon.width - parent.spacing
                        text: qsTr("This color is hard to tell apart from the background. Buttons and selections may be hard to see.")
                        size: "small"
                        wrapMode: Text.Wrap
                        elide: Text.ElideNone
                        horizontalAlignment: Text.AlignLeft
                        Accessible.ignored: true
                    }
                }
            }
        }

        SettingsRow {
            label: qsTr("Density")
            helpText: qsTr("Compact uses smaller controls and rows to fit more on screen.")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("density")
                labels: ({
                        comfortable: qsTr("Comfortable"),
                        compact: qsTr("Compact")
                    })
                value: AppSettings.density
                Accessible.name: qsTr("Density")
                onPicked: value => AppSettings.density = value
            }
        }

        SettingsRow {
            label: qsTr("UI scale")
            helpText: qsTr("Zooms text and controls. Applied when you release the slider.")

            Row {
                width: parent.width
                spacing: Theme.spacingMd

                OsSlider {
                    id: scaleSlider

                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(Theme.spacingXxl * 3,
                                    Math.min(Theme.spacingXxl * 8,
                                             parent.width - scaleText.width - resetScale.width - 2 * parent.spacing))
                    from: 0.8
                    to: 1.5
                    stepSize: 0.05
                    snapMode: T.Slider.SnapAlways
                    value: AppSettings.uiScale

                    Accessible.name: qsTr("UI scale")
                    Accessible.description: scaleText.text

                    // The slider is also "pressed" while an arrow key is down, so each release
                    // (mouse or key, including key auto-repeat) restarts a short delay and the
                    // scale is saved once, when the user stops.
                    onPressedChanged: {
                        if (pressed)
                            commitDelay.stop();
                        else
                            commitDelay.restart();
                    }
                }

                OsText {
                    id: scaleText

                    anchors.verticalCenter: parent.verticalCenter
                    // Tabular digits and a fixed width, so the row doesn't shift while dragging.
                    width: Math.ceil(scaleMetrics.advanceWidth) + Theme.spacingXs
                    text: qsTr("%1%").arg(Math.round(scaleSlider.value * 100))
                    horizontalAlignment: Text.AlignRight
                    elide: Text.ElideNone
                    font.features: ({
                            "tnum": 1
                        })

                    TextMetrics {
                        id: scaleMetrics

                        font: scaleText.font
                        text: qsTr("%1%").arg(150)
                    }
                }

                OsIconButton {
                    id: resetScale

                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "rotate-ccw"
                    toolTip: qsTr("Reset to 100%")
                    enabled: Math.abs(AppSettings.uiScale - 1) > 0.001
                    onClicked: {
                        // This button disables itself: hand the focus to the slider first.
                        if (activeFocus)
                            scaleSlider.forceActiveFocus(Qt.TabFocusReason);
                        commitDelay.stop();
                        AppSettings.uiScale = 1;
                    }
                }

                Timer {
                    id: commitDelay

                    interval: 300
                    onTriggered: page.commitScale()
                }
            }
        }

        SettingsRow {
            label: qsTr("Interface font")
            helpText: qsTr("The font of menus, labels and buttons. The terminal font is set in the terminal settings.")

            OsFontPicker {
                id: fontPicker

                width: Math.min(parent.width, Theme.spacingXxl * 9)
                model: [page.defaultFontLabel].concat(Platform.fontFamilies(false))
                font.family: AppSettings.uiFont.length > 0 ? AppSettings.uiFont : Theme.fontFamily
                Accessible.name: qsTr("Interface font")
                onActivated: index => AppSettings.uiFont = index <= 0 ? "" : textAt(index)
            }
        }

        SettingsRow {
            label: qsTr("Reduce motion")
            helpText: qsTr("Turns off animations and transitions.")

            OsSwitch {
                checked: AppSettings.reduceMotion
                Accessible.name: qsTr("Reduce motion")
                onToggled: AppSettings.reduceMotion = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Layout")

        SettingsRow {
            label: qsTr("Navigation rail")
            helpText: qsTr("When hidden, every view stays reachable from the command palette.")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("railPosition")
                labels: ({
                        left: qsTr("Left"),
                        right: qsTr("Right"),
                        hidden: qsTr("Hidden")
                    })
                icons: ({
                        left: "panel-left",
                        right: "panel-right",
                        hidden: "eye-off"
                    })
                value: AppSettings.railPosition
                Accessible.name: qsTr("Navigation rail")
                onPicked: value => AppSettings.railPosition = value
            }
        }

        SettingsRow {
            label: qsTr("Rail labels")
            helpText: qsTr("Shows the name of each view next to its icon.")

            OsSwitch {
                checked: AppSettings.railLabels
                enabled: AppSettings.railPosition !== "hidden"
                Accessible.name: qsTr("Rail labels")
                onToggled: AppSettings.railLabels = checked
            }
        }

        SettingsRow {
            label: qsTr("Side panel")
            helpText: qsTr("The collapsible panel with SFTP, host details and snippets.")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("sidePanelPosition")
                labels: ({
                        right: qsTr("Right"),
                        left: qsTr("Left")
                    })
                icons: ({
                        right: "panel-right",
                        left: "panel-left"
                    })
                value: AppSettings.sidePanelPosition
                Accessible.name: qsTr("Side panel")
                onPicked: value => AppSettings.sidePanelPosition = value
            }
        }

        SettingsRow {
            label: qsTr("Tabs")

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("tabsPosition")
                labels: ({
                        title_bar: qsTr("In the title bar"),
                        below_title_bar: qsTr("Below the title bar")
                    })
                value: AppSettings.tabsPosition
                Accessible.name: qsTr("Tabs")
                onPicked: value => AppSettings.tabsPosition = value
            }
        }

        SettingsRow {
            label: qsTr("Status bar")
            helpText: qsTr("The bar at the bottom of the window with session details.")

            OsSwitch {
                checked: AppSettings.showStatusBar
                Accessible.name: qsTr("Status bar")
                onToggled: AppSettings.showStatusBar = checked
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Window")

        SettingsRow {
            label: qsTr("Window decorations")
            helpText: {
                const effective = page.decorationName(Platform.effectiveDecorations(AppSettings.windowDecorations));
                const inUse = Platform.desktopName.length > 0
                        ? qsTr("In use now: %1 (detected desktop: %2).").arg(effective).arg(Platform.desktopName)
                        : qsTr("In use now: %1.").arg(effective);
                return [qsTr("Auto uses the OpenSesh title bar, without window buttons on tiling window managers."),
                        inUse, qsTr("Some changes apply only after restarting OpenSesh.")].join(" ");
            }

            SettingsChoice {
                width: parent.width
                values: AppSettings.choices("windowDecorations")
                labels: ({
                        auto: qsTr("Auto"),
                        custom: qsTr("Custom"),
                        native: qsTr("Native"),
                        none: qsTr("None")
                    })
                value: AppSettings.windowDecorations
                Accessible.name: qsTr("Window decorations")
                onPicked: value => AppSettings.windowDecorations = value
            }
        }
    }

    // The pickers change their own value when used; these keep them in step with the settings
    // (for example after "Use the default accent", "Restore defaults" or an edit of the file).
    Binding {
        target: accentPicker
        property: "value"
        value: Theme.accent
    }
    Binding {
        target: fontPicker
        property: "currentFamily"
        value: AppSettings.uiFont.length > 0 ? AppSettings.uiFont : page.defaultFontLabel
    }
}
