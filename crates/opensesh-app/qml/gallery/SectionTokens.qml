// Gallery section: every Theme color token as a swatch (grouped: surfaces, text, accent, status,
// states) with its name, hex value, use and measured contrast, plus the WCAG rules the palette
// guarantees and the current theme flags. Give it a width; the height is implicit.
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Column {
    id: section

    // As many tiles per row as fit at the minimum width (five on a default window).
    readonly property int columns: Math.max(1, Math.floor((width + Theme.spacingLg) / (Theme.spacingXxl * 5.5 + Theme.spacingLg)))
    readonly property real tileWidth: Math.floor((width - (columns - 1) * Theme.spacingLg) / columns)
    readonly property var surfaces: [Theme.bg, Theme.surface, Theme.surface2]

    // WCAG 2.1 relative luminance of an opaque color.
    function luminance(c: color): real {
        const channel = v => v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
        return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
    }

    // WCAG contrast ratio between two opaque colors (1 to 21).
    function contrast(a: color, b: color): real {
        const la = luminance(a);
        const lb = luminance(b);
        return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
    }

    // Lowest contrast of `c` against every color in `backgrounds`.
    function minContrast(c: color, backgrounds: var): real {
        let lowest = 21;
        for (const background of backgrounds)
            lowest = Math.min(lowest, contrast(c, background));
        return lowest;
    }

    function hexOf(c: color): string {
        return c.toString().toUpperCase();
    }

    // Model entry for a tile. `ratio` < 0 means no contrast to report; `minimum` 0 means the
    // token has no required contrast (decorative or disabled).
    function token(name: string, value: color, use: string, ratio: real, against: string,
                   minimum: real): var {
        return {
            name: name,
            value: value,
            use: use,
            ratio: ratio,
            against: against,
            minimum: minimum,
            translucent: value.a < 1
        };
    }

    spacing: Theme.spacingXl

    // A swatch with the token name and hex code in the readable ink for the fill.
    component Tile: Column {
        id: tile

        required property var modelData

        readonly property bool translucent: modelData.translucent
        // Translucent tokens are shown over `surface`, where the UI draws them; text sits on
        // top of them in the normal text color.
        readonly property color ink: translucent ? Theme.text : Theme.textOn(modelData.value)
        readonly property bool passes: modelData.ratio >= modelData.minimum

        width: section.tileWidth
        spacing: Theme.spacingSm

        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("%1, %2").arg(modelData.name).arg(section.hexOf(modelData.value))
        Accessible.description: modelData.use

        Rectangle {
            width: parent.width
            height: Theme.spacingXxl * 2.5
            radius: Theme.radiusCard
            color: tile.translucent ? Theme.surface : tile.modelData.value
            border.width: Theme.borderWidth
            border.color: Theme.border

            Rectangle {
                anchors.fill: parent
                visible: tile.translucent
                radius: parent.radius
                color: tile.modelData.value
            }

            OsText {
                x: Theme.spacingMd
                y: Theme.spacingMd
                width: parent.width - 2 * Theme.spacingMd
                text: tile.modelData.name
                color: tile.ink
                font.weight: Font.DemiBold
                Accessible.ignored: true
            }

            OsText {
                x: Theme.spacingMd
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Theme.spacingMd
                width: parent.width - 2 * Theme.spacingMd
                text: tile.translucent ? qsTr("%1 · %2%").arg(section.hexOf(tile.modelData.value))
                                               .arg(Math.round(tile.modelData.value.a * 100))
                                       : section.hexOf(tile.modelData.value)
                color: tile.ink
                size: "small"
                font.family: Theme.monoFontFamily
                Accessible.ignored: true
            }
        }

        OsText {
            width: parent.width
            text: tile.modelData.use
            size: "small"
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            Accessible.ignored: true
        }

        Row {
            width: parent.width
            spacing: Theme.spacingXs
            visible: tile.modelData.ratio >= 0

            OsIcon {
                id: verdictIcon

                name: tile.modelData.minimum <= 0 ? "info" : tile.passes ? "circle-check" : "triangle-alert"
                color: tile.modelData.minimum <= 0 ? Theme.textMuted : tile.passes ? Theme.success : Theme.warning
                size: Theme.iconSizeSmall
            }

            OsText {
                width: parent.width - verdictIcon.width - parent.spacing
                height: Math.max(implicitHeight, verdictIcon.height)
                text: tile.modelData.minimum > 0
                      ? qsTr("%1:1 %2 (needs %3:1)").arg(tile.modelData.ratio.toFixed(1))
                            .arg(tile.modelData.against).arg(tile.modelData.minimum)
                      : qsTr("%1:1 %2 (no minimum)").arg(tile.modelData.ratio.toFixed(1))
                            .arg(tile.modelData.against)
                size: "small"
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                Accessible.ignored: true
            }
        }
    }

    component Group: Column {
        id: group

        property string title
        property string description
        property var tokens: []

        width: parent ? parent.width : implicitWidth
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: group.title
            description: group.description
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingLg

            Repeater {
                model: group.tokens

                Tile {}
            }
        }
    }

    Column {
        width: parent.width
        spacing: Theme.spacingSm

        OsText {
            text: qsTr("Tokens")
            size: "title"
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            text: qsTr("Every color comes from the Theme singleton, resolved in Rust for the mode, the accent and the density. QML never hardcodes a color.")
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    // Current inputs of the theme.
    Flow {
        width: parent.width
        spacing: Theme.spacingSm

        OsTag {
            text: Theme.dark ? qsTr("Dark") : qsTr("Light")
            iconName: Theme.dark ? "moon" : "sun"
        }
        OsTag {
            text: Theme.compact ? qsTr("Compact") : qsTr("Comfortable")
            iconName: Theme.compact ? "list" : "rows-2"
        }
        OsTag {
            text: qsTr("Accent %1").arg(section.hexOf(Theme.accent))
            iconName: "palette"
            variant: "accent"
        }
        OsTag {
            text: Theme.accentLowContrast ? qsTr("Low-contrast accent") : qsTr("Accent contrast OK")
            iconName: Theme.accentLowContrast ? "triangle-alert" : "circle-check"
        }
        OsTag {
            text: Theme.reduceMotion ? qsTr("Reduce motion on") : qsTr("Animations on")
            iconName: "zap"
        }
    }

    Group {
        title: qsTr("Surfaces")
        description: qsTr("Layers from the window background up, and the lines that separate them.")
        tokens: [
            section.token("bg", Theme.bg, qsTr("Window background"),
                          section.contrast(Theme.text, Theme.bg), qsTr("with text"), 4.5),
            section.token("surface", Theme.surface, qsTr("Cards, panels, popups and dialogs"),
                          section.contrast(Theme.text, Theme.surface), qsTr("with text"), 4.5),
            section.token("surface2", Theme.surface2, qsTr("Inputs, rail, raised and alternate areas"),
                          section.contrast(Theme.text, Theme.surface2), qsTr("with text"), 4.5),
            section.token("border", Theme.border, qsTr("Decorative hairlines and separators"),
                          section.contrast(Theme.border, Theme.surface), qsTr("on surface"), 0),
            section.token("borderStrong", Theme.borderStrong,
                          qsTr("Outlines that identify a control: fields, check boxes, switch tracks"),
                          section.minContrast(Theme.borderStrong, [Theme.surface, Theme.surface2]),
                          qsTr("on surface, surface2"), 3)
        ]
    }

    Group {
        title: qsTr("Text")
        description: qsTr("Ink for text and icons on bg, surface and surface2.")
        tokens: [
            section.token("text", Theme.text, qsTr("Primary text and icons"),
                          section.minContrast(Theme.text, section.surfaces), qsTr("on all surfaces"), 4.5),
            section.token("textMuted", Theme.textMuted, qsTr("Secondary text, placeholders and captions"),
                          section.minContrast(Theme.textMuted, section.surfaces), qsTr("on all surfaces"), 4.5),
            section.token("textDisabled", Theme.textDisabled, qsTr("Disabled text and icons"),
                          section.contrast(Theme.textDisabled, Theme.surface), qsTr("on surface"), 0)
        ]
    }

    Group {
        title: qsTr("Accent")
        description: qsTr("The sesame amber by default; the user can pick any color and the inks follow it.")
        tokens: [
            section.token("accent", Theme.accent,
                          qsTr("Fills: primary buttons, selected indicators, checked states, progress"),
                          section.contrast(Theme.accent, Theme.bg), qsTr("on bg"), 3),
            section.token("accentText", Theme.accentText, qsTr("Text and icons on an accent fill"),
                          section.contrast(Theme.accentText, Theme.accent), qsTr("on accent"), 4.5),
            section.token("accentFg", Theme.accentFg,
                          qsTr("Accent as text or icon color: links, the active rail icon"),
                          section.minContrast(Theme.accentFg, section.surfaces), qsTr("on all surfaces"), 4.5)
        ]
    }

    Group {
        title: qsTr("Status")
        description: qsTr("Fills, borders and icons for messages, badges and validation. Text on them uses Theme.textOn().")
        tokens: [
            section.token("success", Theme.success, qsTr("Success"),
                          section.contrast(Theme.success, Theme.surface), qsTr("on surface"), 3),
            section.token("warning", Theme.warning, qsTr("Warning"),
                          section.contrast(Theme.warning, Theme.surface), qsTr("on surface"), 3),
            section.token("danger", Theme.danger, qsTr("Danger, errors and destructive actions"),
                          section.contrast(Theme.danger, Theme.surface), qsTr("on surface"), 3),
            section.token("info", Theme.info, qsTr("Information"),
                          section.contrast(Theme.info, Theme.surface), qsTr("on surface"), 3)
        ]
    }

    Group {
        title: qsTr("States")
        description: qsTr("Focus, hover, press and selection feedback, and the layer behind modal popups. The translucent ones are shown over surface.")
        tokens: [
            section.token("focusRing", Theme.focusRing, qsTr("Keyboard focus indicator"),
                          section.minContrast(Theme.focusRing, section.surfaces), qsTr("on all surfaces"), 3),
            section.token("hover", Theme.hover, qsTr("Overlay on hovered items"), -1, "", 0),
            section.token("pressed", Theme.pressed, qsTr("Overlay on pressed items"), -1, "", 0),
            section.token("selection", Theme.selection, qsTr("Text selection and selected rows"), -1, "", 0),
            section.token("scrim", Theme.scrim, qsTr("Dimming layer behind dialogs and drawers"), -1, "", 0)
        ]
    }

    // The contrast rules (ADR 0006), enforced by the opensesh-core unit tests.
    OsCard {
        width: parent.width

        Row {
            width: parent.width
            spacing: Theme.spacingMd

            OsIcon {
                id: noteIcon

                name: "shield-check"
                color: Theme.info
                size: Theme.iconSize
            }

            Column {
                width: parent.width - noteIcon.width - parent.spacing
                spacing: Theme.spacingSm

                OsText {
                    width: parent.width
                    text: qsTr("Contrast (WCAG 2.1 AA)")
                    font.weight: Font.DemiBold
                }

                Repeater {
                    model: [
                        qsTr("text, textMuted and accentFg reach 4.5:1 on bg, surface and surface2."),
                        qsTr("borderStrong, focusRing and the status colors reach 3:1 on surface (non-text contrast)."),
                        qsTr("Text on any fill uses Theme.textOn(fill), which picks the more readable ink; accentText is textOn(accent), so any user accent stays readable."),
                        qsTr("border is decorative and textDisabled marks disabled controls, so neither has a minimum."),
                        qsTr("An accent under 3:1 against bg sets Theme.accentLowContrast, and Settings shows a warning.")
                    ]

                    Row {
                        id: rule

                        required property string modelData

                        width: parent.width
                        spacing: Theme.spacingSm

                        OsIcon {
                            id: ruleIcon

                            y: Math.round((Theme.fontSize * 1.2 - height) / 2)
                            name: "check"
                            color: Theme.textMuted
                            size: Theme.iconSizeSmall
                        }

                        OsText {
                            width: parent.width - ruleIcon.width - parent.spacing
                            text: rule.modelData
                            wrapMode: Text.Wrap
                            elide: Text.ElideNone
                        }
                    }
                }

                OsText {
                    width: parent.width
                    text: qsTr("The figures above are measured live on the current theme.")
                    size: "small"
                    muted: true
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
                }
            }
        }
    }
}
