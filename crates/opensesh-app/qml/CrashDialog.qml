// Crash dialog, shown by `opensesh-app --crash-report <file>` after another OpenSesh process
// panicked (see src/crash.rs). Plain QML with the Os components: no QtWidgets. Escape closes it;
// with --smoke-test it exits after the first frame.
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Window {
    id: dialog

    width: 720
    height: 480
    minimumWidth: 420
    minimumHeight: 300
    visible: true
    title: qsTr("OpenSesh crashed")
    color: Theme.bg

    // --crash-report <file> --smoke-test: quit once the dialog has rendered a frame (CI check
    // that the dialog loads on every platform).
    onFrameSwapped: {
        if (AppInfo.smokeTest) {
            console.info("smoke test: crash dialog rendered on", Qt.platform.pluginName);
            Qt.callLater(Qt.exit, 0);
        }
    }

    Component.onCompleted: {
        if (AppInfo.screenshotDir.length > 0 && !AppInfo.smokeTest)
            screenshots.start();
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        onActivated: Qt.quit()
    }

    ThemeBinder {
        id: themeBinder
    }

    Rectangle {
        id: root

        anchors.fill: parent
        color: Theme.bg

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacingXl
            spacing: Theme.spacingMd

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacingMd

                OsIcon {
                    Layout.alignment: Qt.AlignTop
                    name: "triangle-alert"
                    color: Theme.danger
                    size: Theme.iconSize + Theme.spacingXs
                }

                OsText {
                    Layout.fillWidth: true
                    text: qsTr("OpenSesh stopped because of an unexpected error.")
                    size: "large"
                    wrapMode: Text.Wrap
                    elide: Text.ElideNone
                    Accessible.role: Accessible.Heading
                }
            }

            OsText {
                Layout.fillWidth: true
                // The path is data, never markup.
                textFormat: Text.PlainText
                // Paths have no word breaks; Wrap also breaks inside them instead of overflowing.
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                muted: true
                text: qsTr("The details below were saved to %1. Please include them if you report the problem.").arg(AppInfo.crashReportPath)
            }

            T.ScrollView {
                id: scroll

                Layout.fillWidth: true
                Layout.fillHeight: true
                implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                                        contentWidth + leftPadding + rightPadding)
                implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                                         contentHeight + topPadding + bottomPadding)
                padding: Theme.borderWidth

                T.ScrollBar.vertical: OsScrollBar {
                    parent: scroll
                    x: scroll.mirrored ? 0 : scroll.width - width
                    y: scroll.topPadding
                    height: scroll.availableHeight
                    active: scroll.T.ScrollBar.horizontal.active
                }

                T.ScrollBar.horizontal: OsScrollBar {
                    parent: scroll
                    x: scroll.leftPadding
                    y: scroll.height - height
                    width: scroll.availableWidth
                    active: scroll.T.ScrollBar.vertical.active
                }

                T.TextArea {
                    id: details

                    readOnly: true
                    selectByMouse: true
                    persistentSelection: true
                    wrapMode: TextEdit.NoWrap
                    textFormat: TextEdit.PlainText
                    text: AppInfo.crashReport
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.text
                    font.family: Theme.monoFontFamily
                    font.pixelSize: Theme.fontSizeSmall
                    padding: Theme.spacingMd
                    activeFocusOnTab: true
                    Accessible.name: qsTr("Crash details")
                    Accessible.readOnly: true
                }

                // Like a text field: an accent outline while the details have the focus.
                background: Rectangle {
                    color: Theme.surface2
                    radius: Theme.radiusControl
                    border.width: details.activeFocus ? Theme.focusRingWidth : Theme.borderWidth
                    border.color: details.activeFocus ? Theme.accent : Theme.borderStrong
                }
            }

            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: Theme.spacingSm

                OsButton {
                    id: copyButton

                    property bool copied: false

                    text: copied ? qsTr("Copied") : qsTr("Copy details")
                    iconName: copied ? "check" : "copy"
                    onClicked: {
                        details.selectAll();
                        details.copy();
                        details.deselect();
                        copied = true;
                        copiedTimer.restart();
                    }

                    Timer {
                        id: copiedTimer

                        interval: 2000
                        onTriggered: copyButton.copied = false
                    }
                }

                OsButton {
                    text: qsTr("Open logs folder")
                    iconName: "folder-open"
                    onClicked: Qt.openUrlExternally(AppInfo.logsFolder)
                }

                OsButton {
                    text: qsTr("Close")
                    variant: "primary"
                    focus: true
                    onClicked: Qt.quit()
                }
            }
        }
    }

    ScreenshotRunner {
        id: screenshots

        target: root
        binder: themeBinder
        prefix: "crash"
        onFinished: Qt.exit(screenshots.failures > 0 ? 7 : 0)
    }
}
