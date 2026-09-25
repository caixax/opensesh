// Crash dialog, shown by `opensesh-app --crash-report <file>` after another OpenSesh process
// panicked (see src/crash.rs). Plain QML: no QtWidgets.
import QtQuick
// Fusion follows the system light/dark palette on every platform. Placeholder until the
// OpenSesh component library (Sprint 1).
import QtQuick.Controls.Fusion
import QtQuick.Layouts
import cc.caixa.opensesh

ApplicationWindow {
    id: dialog

    width: 720
    height: 480
    minimumWidth: 420
    minimumHeight: 300
    visible: true
    title: qsTr("OpenSesh crashed")

    // --crash-report <file> --smoke-test: quit once the dialog has rendered a frame (CI check
    // that the dialog loads on every platform).
    onFrameSwapped: {
        if (AppInfo.smokeTest) {
            console.info("smoke test: crash dialog rendered on", Qt.platform.pluginName);
            Qt.callLater(Qt.exit, 0);
        }
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        onActivated: Qt.quit()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            font.bold: true
            text: qsTr("OpenSesh stopped because of an unexpected error.")
        }

        Label {
            Layout.fillWidth: true
            // Paths have no word breaks; Wrap also breaks inside them instead of overflowing.
            wrapMode: Text.Wrap
            // The path is data, never markup.
            textFormat: Text.PlainText
            text: qsTr("The details below were saved to %1. Please include them if you report the problem.").arg(AppInfo.crashReportPath)
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true

            TextArea {
                id: details

                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.NoWrap
                text: AppInfo.crashReport
                Accessible.name: qsTr("Crash details")
            }
        }

        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 8

            Button {
                text: qsTr("Copy details")
                onClicked: {
                    details.selectAll();
                    details.copy();
                    details.deselect();
                }
            }

            Button {
                text: qsTr("Open logs folder")
                onClicked: Qt.openUrlExternally(AppInfo.logsFolder)
            }

            Button {
                text: qsTr("Close")
                highlighted: true
                focus: true
                onClicked: Qt.quit()
            }
        }
    }
}
