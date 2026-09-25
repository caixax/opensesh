// Component gallery (`--gallery`): every Os* component in its states, in six sections (Tokens,
// Typography and spacing, Inputs, Structure, Overlays, Terminal) behind a navigation rail. The
// toolbar switches the gallery's own theme, density, accent and reduce motion through the
// ThemeBinder overrides; the user's settings are never written.
// `--smoke-test` visits every section, opens and closes every overlay, flips each switch and
// drives the terminal renderer demo.
// `--screenshots <dir>` captures every section, plus the open overlays and the custom accent
// dialog, in each theme x density combination as gallery-<page>-<mode>-<density>.png; for a
// section page the window grows to fit the whole section.
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Templates as T
import cc.caixa.opensesh

Window {
    id: window

    readonly property bool screenshotMode: AppInfo.screenshotDir.length > 0
    readonly property var sections: [
        { id: "tokens", text: qsTr("Tokens"), iconName: "palette" },
        { id: "typography", text: qsTr("Typography and spacing"), iconName: "type" },
        { id: "inputs", text: qsTr("Inputs"), iconName: "sliders-horizontal" },
        { id: "structure", text: qsTr("Structure"), iconName: "columns-2" },
        { id: "overlays", text: qsTr("Overlays"), iconName: "app-window" },
        { id: "terminal", text: qsTr("Terminal"), iconName: "square-terminal" }
    ]
    property string currentSection: "tokens"
    readonly property Item currentItem: {
        switch (currentSection) {
        case "typography":
            return typographySection;
        case "inputs":
            return inputsSection;
        case "structure":
            return structureSection;
        case "overlays":
            return overlaysSection;
        case "terminal":
            return terminalSection;
        default:
            return tokensSection;
        }
    }
    // Screenshots: grow the window so the whole section is captured, not only the viewport.
    property bool fitToContent: false

    function showSection(id: string) {
        currentSection = id;
        nav.currentId = id;
        flick.contentY = 0;
        // Hiding an item doesn't take its focus away: move it off the hidden section, or its
        // focus-driven popups (rail tooltips) would stay on screen.
        const focused = window.activeFocusItem;
        if (focused && !focused.visible)
            nav.forceActiveFocus(Qt.OtherFocusReason);
    }

    // Scrolls the section so that the item with the keyboard focus is visible (Tab into a control
    // below the fold), as SettingsView does.
    function revealFocusedItem() {
        const item = window.activeFocusItem;
        if (!item)
            return;
        let ancestor = item.parent;
        while (ancestor && ancestor !== page)
            ancestor = ancestor.parent;
        if (!ancestor)
            return;
        const margin = Theme.spacingLg;
        const top = item.mapToItem(flick.contentItem, 0, 0).y;
        const bottom = top + item.height;
        const maxY = Math.max(0, flick.contentHeight - flick.height);
        if (top - margin < flick.contentY)
            flick.contentY = Math.max(0, top - margin);
        else if (bottom + margin > flick.contentY + flick.height)
            flick.contentY = Math.min(maxY, bottom + margin - flick.height);
    }

    // Screenshot pages: a section id, optionally followed by an overlay to open ("overlays-menu").
    function preparePage(page: string) {
        overlaysSection.closeAll();
        toolbar.closeAccentDialog();
        toastHost.clear();
        const dash = page.indexOf("-");
        const section = dash < 0 ? page : page.substring(0, dash);
        const overlay = dash < 0 ? "" : page.substring(dash + 1);
        showSection(section);
        fitToContent = overlay.length === 0;
        switch (overlay) {
        case "dialog":
            overlaysSection.openDialog();
            break;
        case "menu":
            overlaysSection.openMenu(true);
            break;
        case "drawer":
            overlaysSection.openDrawer();
            break;
        case "accent":
            toolbar.openAccentDialog();
            break;
        case "toasts":
            overlaysSection.showSampleToasts();
            break;
        case "palette":
            overlaysSection.openPalette("");
            break;
        default:
            // Show one keyboard focus ring per page where the section has a demo for it.
            Qt.callLater(() => {
                if (window.currentItem.showFocus !== undefined)
                    window.currentItem.showFocus();
            });
        }
    }

    width: 1280
    height: 900
    minimumWidth: 800
    minimumHeight: 560
    visible: true
    title: qsTr("OpenSesh component gallery")
    color: Theme.bg

    onActiveFocusItemChanged: revealFocusedItem()

    Component.onCompleted: {
        // Start from the user's current look, then keep every change local to the gallery.
        themeBinder.overrideMode = Theme.dark ? "dark" : "light";
        themeBinder.overrideDensity = Theme.compact ? "compact" : "comfortable";
        themeBinder.overrideAccent = screenshotMode ? "default" : AppSettings.accent;
        themeBinder.overrideReduceMotion = screenshotMode ? false : AppSettings.reduceMotion;
        themeBinder.overrideActive = true;
        if (screenshotMode)
            screenshots.start();
    }

    ThemeBinder {
        id: themeBinder
    }

    Binding {
        target: window
        property: "height"
        when: window.screenshotMode && window.fitToContent
        value: Math.ceil(toolbar.height + Math.max(page.height + 2 * Theme.spacingXl, nav.implicitHeight))
    }

    Rectangle {
        id: root

        anchors.fill: parent
        color: Theme.bg

        GalleryToolbar {
            id: toolbar

            width: parent.width
            binder: themeBinder
        }

        OsRail {
            id: nav

            anchors.top: toolbar.bottom
            anchors.bottom: parent.bottom
            // Wider than a shell rail so "Typography and spacing" is not elided.
            width: Theme.railWidthLabels + Theme.spacingXxl + Theme.spacingXl
            showLabels: true
            model: window.sections
            currentId: window.currentSection
            Accessible.name: qsTr("Gallery sections")
            onActivated: id => window.showSection(id)
        }

        Flickable {
            id: flick

            anchors.top: toolbar.bottom
            anchors.bottom: parent.bottom
            anchors.left: nav.right
            anchors.right: parent.right
            contentWidth: width
            contentHeight: page.height + 2 * Theme.spacingXl
            boundsBehavior: Flickable.StopAtBounds
            clip: true

            Keys.onPressed: event => {
                const step = event.key === Qt.Key_PageDown ? height * 0.9
                           : event.key === Qt.Key_PageUp ? -height * 0.9 : 0;
                if (step === 0)
                    return;
                contentY = Math.max(0, Math.min(contentHeight - height, contentY + step));
                event.accepted = true;
            }

            Item {
                id: page

                x: Theme.spacingXl
                y: Theme.spacingXl
                width: flick.width - 2 * Theme.spacingXl
                height: window.currentItem.implicitHeight

                SectionTokens {
                    id: tokensSection

                    width: parent.width
                    visible: window.currentSection === "tokens"
                }

                SectionTypography {
                    id: typographySection

                    width: parent.width
                    visible: window.currentSection === "typography"
                }

                SectionInputs {
                    id: inputsSection

                    width: parent.width
                    visible: window.currentSection === "inputs"
                }

                SectionStructure {
                    id: structureSection

                    width: parent.width
                    visible: window.currentSection === "structure"
                }

                SectionOverlays {
                    id: overlaysSection

                    width: parent.width
                    visible: window.currentSection === "overlays"
                    // Part of the section inside the viewport (the pinned tooltip hides outside it).
                    visibleTop: flick.contentY - page.y
                    visibleBottom: flick.contentY - page.y + flick.height
                }

                SectionTerminal {
                    id: terminalSection

                    width: parent.width
                    visible: window.currentSection === "terminal"
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }

        // Anchors itself to the bottom-right corner of its parent.
        OsToastHost {
            id: toastHost
        }
    }

    SmokeTest {
        window: window
        steps: [
            () => window.showSection("typography"),
            () => window.showSection("inputs"),
            () => window.showSection("structure"),
            () => window.showSection("overlays")
        ].concat(overlaysSection.smokeSteps).concat([
            () => toastHost.clear(),
            () => window.showSection("terminal")
        ]).concat(terminalSection.smokeSteps).concat([
            () => toolbar.setMode(Theme.dark ? "light" : "dark"),
            () => toolbar.setDensity(Theme.compact ? "comfortable" : "compact"),
            () => toolbar.setAccent(Theme.accentPresets[4]),
            () => toolbar.openAccentDialog(),
            () => toolbar.closeAccentDialog(),
            () => toolbar.setReduceMotion(true),
            () => window.showSection("tokens")
        ])
    }

    ScreenshotRunner {
        id: screenshots

        target: window.contentItem
        binder: themeBinder
        prefix: "gallery"
        pages: ["tokens", "typography", "inputs", "structure", "overlays", "overlays-dialog",
            "overlays-menu", "overlays-drawer", "overlays-palette", "overlays-toasts", "tokens-accent",
            "terminal"]
        prepare: (mode, density, page) => window.preparePage(page)
        onFinished: Qt.exit(screenshots.failures > 0 ? 7 : 0)
    }
}
