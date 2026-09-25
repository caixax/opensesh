// `--screenshots <dir>`: captures `target` in every theme x density combination as
// <dir>/<prefix>-<mode>-<density>.png, then emits `finished`.
import QtQuick
import cc.caixa.opensesh

Item {
    id: runner

    required property Item target
    required property ThemeBinder binder
    property string prefix: "main"
    // Called before each capture with (mode, density), e.g. to open a view or popup.
    property var prepare: null

    readonly property var combos: [["dark", "comfortable"], ["dark", "compact"],
        ["light", "comfortable"], ["light", "compact"]]
    property int index: -1

    signal finished

    visible: false

    function start() {
        binder.overrideActive = true;
        next();
    }

    function next() {
        index += 1;
        if (index >= combos.length) {
            finished();
            return;
        }
        binder.overrideMode = combos[index][0];
        binder.overrideDensity = combos[index][1];
        if (prepare)
            prepare(combos[index][0], combos[index][1]);
        settle.restart();
    }

    // Leave time for layout, icon loading and a couple of frames before grabbing.
    Timer {
        id: settle

        interval: 400
        onTriggered: {
            const file = AppInfo.screenshotDir + "/" + runner.prefix + "-" + runner.combos[runner.index][0]
                    + "-" + runner.combos[runner.index][1] + ".png";
            const started = runner.target.grabToImage(result => {
                if (!result.saveToFile(file))
                    console.warn("screenshot: could not save", file);
                else
                    console.info("screenshot:", file);
                runner.next();
            });
            if (!started) {
                console.warn("screenshot: grabToImage failed");
                runner.next();
            }
        }
    }
}
