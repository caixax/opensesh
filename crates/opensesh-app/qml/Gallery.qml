// Component gallery (`--gallery`). (Temporary scaffold: completed in this sprint.)
import QtQuick
import cc.caixa.opensesh

Window {
    id: window

    width: 1200
    height: 800
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

        Row {
            anchors.centerIn: parent
            spacing: Theme.spacingMd

            Repeater {
                model: ["primary", "secondary", "ghost", "danger"]

                OsButton {
                    required property string modelData

                    text: qsTr("Button")
                    variant: modelData
                }
            }
        }
    }

    SmokeTest {
        window: window
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
