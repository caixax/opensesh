// A small picture of a terminal theme drawn from its colors (the `colors` object of the
// TerminalProfiles themes JSON): a prompt, a command with its output, a selection, the cursor and
// the 16 ANSI colors. Used by the theme list and the theme editor.
//   colors: var      the theme's colors
//   compact: bool    only the prompt line and the color strip (theme cards)
import QtQuick
import cc.caixa.opensesh

Rectangle {
    id: sample

    property var colors: null
    property bool compact: false

    readonly property bool ready: colors !== null && colors !== undefined && colors.normal !== undefined

    implicitWidth: Theme.spacingXxl * 10
    implicitHeight: content.implicitHeight + 2 * Theme.spacingSm
    radius: Theme.radiusControl
    color: ready ? colors.background : Theme.surface2
    border.width: Theme.borderWidth
    border.color: Theme.border
    clip: true

    Accessible.role: Accessible.Graphic
    Accessible.name: qsTr("Theme sample")

    Column {
        id: content

        visible: sample.ready
        x: Theme.spacingSm
        y: Theme.spacingSm
        width: parent.width - 2 * Theme.spacingSm
        spacing: Theme.spacingXs

        Row {
            spacing: 0

            Text {
                text: "user@host" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.normal[2] : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                font.bold: true
                Accessible.ignored: true
            }

            Text {
                text: ":" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.foreground : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.ignored: true
            }

            Text {
                text: "~/src" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.normal[4] : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                font.bold: true
                Accessible.ignored: true
            }

            Text {
                text: "$ ls" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.foreground : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.ignored: true
            }

            Rectangle {
                width: cursorMetrics.advanceWidth
                height: cursorMetrics.height
                color: sample.ready ? sample.colors.cursor : sample.color

                TextMetrics {
                    id: cursorMetrics

                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    text: "M" // lint-qml: allow (sizes the cursor block, never shown)
                }
            }
        }

        Row {
            visible: !sample.compact
            spacing: Theme.spacingMd

            Text {
                text: "docs" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.normal[4] : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                font.bold: true
                Accessible.ignored: true
            }

            Text {
                text: "run.sh" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.normal[2] : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.ignored: true
            }

            Rectangle {
                width: selectedText.implicitWidth
                height: selectedText.implicitHeight
                color: sample.ready ? sample.colors.selectionBackground : sample.color

                Text {
                    id: selectedText

                    text: "notes.md" // lint-qml: allow (sample terminal output)
                    color: sample.ready ? (sample.colors.selectionForeground.length > 0 ? sample.colors.selectionForeground : sample.colors.foreground) : sample.color
                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    Accessible.ignored: true
                }
            }

            Text {
                text: "error.log" // lint-qml: allow (sample terminal output)
                color: sample.ready ? sample.colors.normal[1] : sample.color
                font.family: Theme.monoFontFamily
                font.pixelSize: Theme.fontSizeSmall
                Accessible.ignored: true
            }
        }

        Row {
            spacing: 0

            Repeater {
                model: sample.ready ? sample.colors.normal.concat(sample.colors.bright) : []

                delegate: Rectangle {
                    required property string modelData

                    width: Math.floor(content.width / 16)
                    height: sample.compact ? Theme.spacingSm : Theme.spacingMd
                    color: modelData
                }
            }
        }
    }
}
