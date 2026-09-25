// Component gallery (`--gallery`). (Provisional layout: every section in one scrolling column;
// the full gallery with theme/density switches is completed later in this sprint.)
import QtQuick
import cc.caixa.opensesh

Window {
    id: window

    width: 1280
    height: 900
    visible: true
    title: qsTr("OpenSesh component gallery")
    color: Theme.bg

    ThemeBinder {
        id: themeBinder
    }

    Rectangle {
        id: root

        anchors.fill: parent
        color: Theme.bg

        Flickable {
            id: flick

            anchors.fill: parent
            contentWidth: width
            contentHeight: column.implicitHeight + Theme.spacingXxl * 2
            clip: true

            Column {
                id: column

                x: Theme.spacingXl
                y: Theme.spacingXl
                width: flick.width - Theme.spacingXl * 2
                spacing: Theme.spacingXxl

                SectionInputs {
                    width: parent.width
                    focusDemo: false
                }

                SectionStructure {
                    width: parent.width
                }

                SectionOverlays {
                    id: overlays

                    width: parent.width
                    pinTooltip: false
                }
            }

            OsScrollBar.vertical: OsScrollBar {}
        }

        OsToastHost {
            anchors.fill: parent
        }
    }

    SmokeTest {
        window: window
        steps: overlays.smokeSteps
    }

    ScreenshotRunner {
        id: screenshots

        target: root
        binder: themeBinder
        prefix: "gallery"
        onFinished: Qt.exit(0)
    }

    Component.onCompleted: {
        if (AppInfo.screenshotDir.length > 0)
            screenshots.start();
    }
}
