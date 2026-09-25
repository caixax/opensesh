// `--smoke-test`: after the first frame, runs `steps` (one per ~frame, e.g. open every view so
// its QML is instantiated), then checks the Rust bridges and the text encoding and exits.
// Exit codes: 0 ok, 3 no frame rendered, 4 bridge broken, 5 text encoding broken. main.rs turns
// a clean exit into 6 if our QML logged any warning.
import QtQuick
import cc.caixa.opensesh

Item {
    id: smoke

    required property Window window
    // Functions run in order, one every `stepInterval` ms, before the checks.
    property var steps: []
    property int stepInterval: 60
    property bool started: false
    property int stepIndex: 0

    visible: false

    // Keep this function typed and literal-only: that is what makes qmlcachegen compile it to
    // C++, where a wrong source encoding turns the 3-character literal into 7 characters.
    function textEncodingOk(): bool {
        return "·ñ€".length === 3;
    }

    function bridgeOk() {
        return AppSettings.choices("density").length === 2
                && Platform.languages().length >= 2
                && Theme.textOn(Theme.accent) === Theme.accentText
                && Theme.controlHeight > 0;
    }

    function finish() {
        const bridge = bridgeOk();
        const encoding = textEncodingOk();
        console.info("smoke test: first frame rendered on", Qt.platform.pluginName,
                     "- QML/Rust bridge", bridge ? "ok" : "BROKEN",
                     "- text encoding", encoding ? "ok" : "BROKEN",
                     "- steps", smoke.steps.length);
        Qt.exit(!bridge ? 4 : !encoding ? 5 : 0);
    }

    Connections {
        target: smoke.window
        enabled: AppInfo.smokeTest

        function onFrameSwapped() {
            if (smoke.started)
                return;
            smoke.started = true;
            stepTimer.start();
        }
    }

    Timer {
        id: stepTimer

        interval: smoke.stepInterval
        repeat: true
        onTriggered: {
            if (smoke.stepIndex < smoke.steps.length) {
                smoke.steps[smoke.stepIndex]();
                smoke.stepIndex += 1;
            } else {
                stop();
                smoke.finish();
            }
        }
    }

    Timer {
        interval: 20000
        running: AppInfo.smokeTest
        onTriggered: {
            console.error("smoke test: no frame rendered after", interval, "ms");
            Qt.exit(3);
        }
    }
}
