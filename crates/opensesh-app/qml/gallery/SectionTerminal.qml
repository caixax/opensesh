// Gallery section: the terminal renderer (TerminalItem, ADR 0013) drawing its built-in demo frame
// (every color mode, style, underline, wide and combining characters, emoji, box drawing,
// Powerline, selection, search match, link and cursor), the four cursor shapes, and a benchmark
// that redraws every row on every frame (run with OPENSESH_TERMINAL_STATS=1 to log the timings).
// Qt Quick's software backend (QT_QPA_PLATFORM=offscreen) draws only the background.
// Give it a width; the height is implicit.
//   smokeSteps: list   functions for SmokeTest.steps: cursor shapes, font size, benchmark, focus
// Functions: focusTerminal() gives the demo terminal the keyboard focus (not used by screenshots:
// the blinking cursor would make them differ from run to run).
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

Column {
    id: section

    property bool benchmark: false
    // Lines of the demo terminal: the whole demo frame fits.
    readonly property int demoLines: 24
    readonly property var cursorShapes: [qsTr("Block"), qsTr("Hollow block"), qsTr("Beam"),
        qsTr("Underline"), qsTr("Hidden")]
    readonly property var smokeSteps: [
        () => terminal.demoCursorShape = 2,
        () => terminal.demoCursorShape = 1,
        () => terminal.demoCursorShape = 0,
        () => sizeBox.value = 14,
        () => sizeBox.value = 11,
        () => section.benchmark = true,
        () => section.benchmark = false,
        () => section.focusTerminal(),
        () => {
            if (terminal.columns < 1 || terminal.lines < 1 || terminal.cellWidth <= 0)
                console.warn("gallery: the terminal demo has no grid:", terminal.columns, "x",
                             terminal.lines);
        }
    ]

    function focusTerminal() {
        terminal.forceActiveFocus(Qt.TabFocusReason);
    }

    spacing: Theme.spacingXl

    component Label: OsText {
        size: "small"
        muted: true
    }

    Column {
        width: parent.width
        spacing: Theme.spacingSm

        OsText {
            text: qsTr("Terminal renderer")
            size: "title"
            Accessible.role: Accessible.Heading
        }

        OsText {
            width: parent.width
            text: qsTr("The terminal grid is drawn by the scene graph with a glyph atlas. This demo frame shows what the renderer supports; a real session replaces it once the engine is connected.")
            muted: true
            wrapMode: Text.WordWrap
        }
    }

    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Demo frame")
            description: qsTr("Click the terminal or tab into it: the cursor blinks while it has the focus, unless reduce motion is on.")
        }

        Row {
            spacing: Theme.spacingLg

            Column {
                spacing: Theme.spacingXs

                Label {
                    text: qsTr("Cursor")
                }

                OsComboBox {
                    model: section.cursorShapes
                    currentIndex: terminal.demoCursorShape
                    Accessible.name: qsTr("Cursor shape")
                    onActivated: index => terminal.demoCursorShape = index
                }
            }

            Column {
                spacing: Theme.spacingXs

                Label {
                    text: qsTr("Font size (points)")
                }

                OsSpinBox {
                    id: sizeBox

                    from: 6
                    to: 36
                    value: 11
                    Accessible.name: qsTr("Font size")
                }
            }

            OsSwitch {
                anchors.bottom: parent.bottom
                text: qsTr("Benchmark: redraw every row on every frame")
                checked: section.benchmark
                onToggled: section.benchmark = checked
            }
        }

        Rectangle {
            id: frame

            width: parent.width
            height: Math.ceil(terminal.cellHeight * section.demoLines + 2 * terminal.padding)
                    + 2 * Theme.borderWidth
            color: Theme.surface
            radius: Theme.radiusCard
            border.width: Theme.borderWidth
            border.color: terminal.activeFocus ? Theme.focusRing : Theme.border
            clip: true

            TerminalItem {
                id: terminal

                anchors.fill: parent
                anchors.margins: Theme.borderWidth
                demo: true
                demoDark: Theme.dark
                demoAnimated: section.benchmark
                fontFamily: Theme.monoFontFamily
                fontPointSize: sizeBox.value
                padding: Theme.spacingSm
                reduceMotion: Theme.reduceMotion
                Accessible.name: qsTr("Terminal demo")
            }

            // Continuous frames while the benchmark runs.
            FrameAnimation {
                running: section.benchmark && terminal.visible
                onTriggered: terminal.requestFrame()
            }
        }

        Label {
            text: qsTr("%1 × %2 cells of %3 × %4 px").arg(terminal.columns).arg(terminal.lines)
                  .arg(Math.round(terminal.cellWidth * 100) / 100)
                  .arg(Math.round(terminal.cellHeight * 100) / 100)
        }
    }

    Column {
        width: parent.width
        spacing: Theme.spacingMd

        OsSectionHeader {
            width: parent.width
            title: qsTr("Cursor shapes")
            description: qsTr("Block (the character under it is redrawn in the cursor text color), hollow block (unfocused), beam and underline.")
        }

        Row {
            spacing: Theme.spacingLg

            Repeater {
                model: section.cursorShapes.length - 1

                Column {
                    id: shapeDemo

                    required property int index

                    spacing: Theme.spacingXs

                    Rectangle {
                        width: sample.cellWidth * 12 + 2 * sample.padding + 2 * Theme.borderWidth
                        height: sample.cellHeight + 2 * sample.padding + 2 * Theme.borderWidth
                        color: Theme.surface
                        radius: Theme.radiusSmall
                        border.width: Theme.borderWidth
                        border.color: Theme.border
                        clip: true

                        TerminalItem {
                            id: sample

                            anchors.fill: parent
                            anchors.margins: Theme.borderWidth
                            demo: true
                            demoDark: Theme.dark
                            demoCursorShape: shapeDemo.index
                            fontFamily: Theme.monoFontFamily
                            fontPointSize: sizeBox.value
                            padding: Theme.spacingXs
                            reduceMotion: Theme.reduceMotion
                            // Samples only: no focus, no input. (`activeFocusOnTab` can't be set
                            // on TerminalItem from QML with Qt 6.8, see terminal_item.h.)
                            enabled: false
                            Accessible.name: section.cursorShapes[shapeDemo.index]
                        }
                    }

                    Label {
                        text: section.cursorShapes[shapeDemo.index]
                    }
                }
            }
        }
    }
}
