// `--screenshots <dir>`: captures `target` in every theme x density combination as
// <dir>/<prefix>-<mode>-<density>.png, then emits `finished`. With `pages`, it captures every
// page in every combination as <dir>/<prefix>-<page>-<mode>-<density>.png instead.
import QtQuick
import cc.caixa.opensesh

Item {
    id: runner

    required property Item target
    required property ThemeBinder binder
    property string prefix: "main"
    // Called before each capture with (mode, density, page), e.g. to open a view or popup.
    // `page` is "" when `pages` is empty.
    property var prepare: null
    // Optional page names (e.g. gallery sections); each one is captured in every combination.
    property var pages: []

    readonly property var combos: [["dark", "comfortable"], ["dark", "compact"],
        ["light", "comfortable"], ["light", "compact"]]
    readonly property var pageList: pages.length > 0 ? pages : [""]
    // Index into combos x pageList, combination-major.
    property int index: -1

    readonly property int comboIndex: Math.floor(index / pageList.length)
    readonly property string page: index >= 0 ? pageList[index % pageList.length] : ""

    signal finished

    visible: false

    function start() {
        binder.overrideActive = true;
        next();
    }

    function next() {
        index += 1;
        if (index >= combos.length * pageList.length) {
            finished();
            return;
        }
        binder.overrideMode = combos[comboIndex][0];
        binder.overrideDensity = combos[comboIndex][1];
        if (prepare)
            prepare(combos[comboIndex][0], combos[comboIndex][1], page);
        settle.restart();
    }

    // Leave time for layout, icon loading and a couple of frames before grabbing.
    Timer {
        id: settle

        interval: 400
        onTriggered: {
            const name = runner.page.length > 0 ? runner.prefix + "-" + runner.page : runner.prefix;
            const file = AppInfo.screenshotDir + "/" + name + "-" + runner.combos[runner.comboIndex][0]
                    + "-" + runner.combos[runner.comboIndex][1] + ".png";
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
