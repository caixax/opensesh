// Sprint 0 bootstrap window: proves the Rust <-> QML bridge works on every platform.
// No colors are set here on purpose: the Theme singleton arrives in Sprint 1.
import QtQuick
// Fusion follows the system light/dark palette on every platform. Placeholder until the
// OpenSesh component library (Sprint 1).
import QtQuick.Controls.Fusion
import QtQuick.Layouts
import cc.caixa.opensesh

ApplicationWindow {
    id: window

    width: 640
    height: 400
    minimumWidth: 360
    minimumHeight: 260
    visible: true
    title: qsTr("OpenSesh")

    // --smoke-test: once the first frame is on screen, press the button three times and check
    // that the Rust object reacted, and that non-ASCII QML text survived the build (MSVC once
    // mangled it, see build.rs). Exit codes: 0 = ok, 3 = nothing rendered, 4 = bridge broken,
    // 5 = text encoding broken.
    property bool smokeFrameSeen: false

    // Keep this function typed and literal-only: that is what makes qmlcachegen compile it to
    // C++, where a wrong source encoding turns the 3-character literal into 7 characters.
    function textEncodingOk(): bool {
        return "·ñ€".length === 3;
    }

    function runSmokeTest() {
        for (let i = 0; i < 3; ++i)
            knockButton.clicked();
        const bridgeOk = door.knocks === 3 && door.open;
        const encodingOk = window.textEncodingOk();
        console.info("smoke test: first frame rendered on", Qt.platform.pluginName,
                     "- QML/Rust bridge", bridgeOk ? "ok" : "BROKEN",
                     "- text encoding", encodingOk ? "ok" : "BROKEN");
        Qt.exit(!bridgeOk ? 4 : !encodingOk ? 5 : 0);
    }

    onFrameSwapped: {
        if (AppInfo.smokeTest && !window.smokeFrameSeen) {
            window.smokeFrameSeen = true;
            Qt.callLater(window.runSmokeTest);
        }
    }

    Timer {
        interval: 15000
        running: AppInfo.smokeTest
        onTriggered: {
            console.error("smoke test: no frame rendered after", interval, "ms");
            Qt.exit(3);
        }
    }

    SesameDoor {
        id: door
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 480)
        spacing: 16

        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.pixelSize: 24
            text: door.open ? qsTr("Open sesame! The door is open.") : qsTr("The door is closed.")
        }

        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Knocked %n time(s).", "", door.knocks)
        }

        Button {
            id: knockButton

            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Knock")
            focus: true
            Accessible.name: qsTr("Knock on the door")
            Accessible.description: qsTr("Knock three times to open the door.")
            onClicked: door.knock()
        }

        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            opacity: 0.7
            text: qsTr("Version %1 · Qt platform: %2").arg(AppInfo.version).arg(Qt.platform.pluginName)
        }
    }
}
