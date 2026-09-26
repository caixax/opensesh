// Settings > Themes (PLAN §6.3): every terminal theme, built-in and the user's, with a sample of
// each. Pick one for the dark or light scheme of the default profile, import themes from iTerm2,
// Windows Terminal, Alacritty, Kitty or base16 files, export to OpenSesh or Alacritty, and edit
// themes visually (a built-in theme is edited as a copy).
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Dialogs
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property var themeList: JSON.parse(TerminalProfiles.themes || "[]")
    readonly property var defaults: TerminalProfiles.revision >= 0 ? JSON.parse(TerminalProfiles.profileJson("default") || "{}").values || ({}) : ({})
    property string selectedId: Theme.dark ? (defaults.theme_dark || "") : (defaults.theme_light || "")

    readonly property var selected: {
        for (const theme of themeList) {
            if (theme.id === selectedId)
                return theme;
        }
        return null;
    }

    // For the smoke test.
    function openEditor() {
        editor.openTheme(themeList.length > 0 ? themeList[0].id : "");
    }

    function closeEditor() {
        editor.close();
    }

    function useFor(key) {
        const error = TerminalProfiles.setOption("default", key, JSON.stringify(selectedId));
        if (error.length > 0)
            Toasts.show(qsTr("Could not change the theme: %1").arg(error), "warning");
    }

    title: qsTr("Themes")
    description: qsTr("Terminal color themes. The default profile uses one theme while OpenSesh is dark and one while it is light; other profiles can choose their own in Settings > Terminal.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Themes")

        Flow {
            id: grid

            width: parent.width
            spacing: Theme.spacingSm

            Repeater {
                model: page.themeList

                delegate: OsCard {
                    id: card

                    required property var modelData

                    width: Math.floor((grid.width - 2 * grid.spacing) / 3)
                    padding: Theme.spacingSm
                    clickable: true
                    selected: page.selectedId === modelData.id
                    Accessible.name: modelData.name

                    onClicked: page.selectedId = modelData.id

                    Column {
                        width: parent.width
                        spacing: Theme.spacingXs

                        ThemeSample {
                            width: parent.width
                            colors: card.modelData.colors
                            compact: true
                        }

                        OsText {
                            width: parent.width
                            text: card.modelData.name
                            elide: Text.ElideRight
                        }

                        OsText {
                            width: parent.width
                            text: {
                                const uses = [];
                                if (card.modelData.id === page.defaults.theme_dark)
                                    uses.push(qsTr("Dark"));
                                if (card.modelData.id === page.defaults.theme_light)
                                    uses.push(qsTr("Light"));
                                const origin = card.modelData.builtin ? qsTr("Built-in") : qsTr("Yours");
                                return uses.length > 0 ? qsTr("%1 · in use: %2").arg(origin).arg(uses.join(", ")) : origin;
                            }
                            size: "small"
                            muted: true
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: page.selected ? page.selected.name : qsTr("No theme selected")
        description: page.selected && page.selected.author.length > 0
                     ? qsTr("By %1%2").arg(page.selected.author).arg(page.selected.license.length > 0 ? qsTr(", %1 license").arg(page.selected.license) : "")
                     : ""

        ThemeSample {
            width: parent.width
            visible: page.selected !== null
            colors: page.selected ? page.selected.colors : null
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Use when dark")
                iconName: "moon"
                enabled: page.selected !== null && page.selectedId !== page.defaults.theme_dark
                onClicked: page.useFor("theme_dark")
            }

            OsButton {
                text: qsTr("Use when light")
                iconName: "sun"
                enabled: page.selected !== null && page.selectedId !== page.defaults.theme_light
                onClicked: page.useFor("theme_light")
            }

            OsButton {
                text: page.selected && page.selected.builtin ? qsTr("Edit a copy") : qsTr("Edit")
                iconName: "pencil"
                enabled: page.selected !== null
                onClicked: editor.openTheme(page.selectedId)
            }

            OsButton {
                text: qsTr("Duplicate")
                iconName: "copy"
                enabled: page.selected !== null
                onClicked: {
                    const id = TerminalProfiles.duplicateTheme(page.selectedId);
                    if (id.length > 0)
                        page.selectedId = id;
                }
            }

            OsButton {
                text: qsTr("Export…")
                iconName: "upload"
                enabled: page.selected !== null
                onClicked: exportDialog.open()
            }

            OsButton {
                text: qsTr("Delete")
                iconName: "trash-2"
                variant: "danger"
                enabled: page.selected !== null && !page.selected.builtin
                onClicked: {
                    if (TerminalProfiles.deleteTheme(page.selectedId))
                        page.selectedId = Theme.dark ? (page.defaults.theme_dark || "") : (page.defaults.theme_light || "");
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Import")
        description: qsTr("Themes from iTerm2 (.itermcolors), Windows Terminal (.json), Alacritty (.toml), Kitty (.conf) and base16 (.yaml). Colors a file leaves out are filled in to match.")

        OsButton {
            text: qsTr("Import a theme…")
            iconName: "import"
            variant: "primary"
            onClicked: importDialog.open()
        }
    }

    Connections {
        target: TerminalProfiles

        function onThemesImported(ids, error) {
            if (error.length > 0) {
                Toasts.show(qsTr("Could not import the theme: %1").arg(error), "danger");
                return;
            }
            const list = JSON.parse(ids);
            if (list.length > 0)
                page.selectedId = list[0];
            Toasts.show(list.length === 1 ? qsTr("Theme imported.") : qsTr("%1 themes imported.").arg(list.length), "success");
        }

        function onThemeExported(path, error) {
            if (error.length > 0)
                Toasts.show(qsTr("Could not export the theme: %1").arg(error), "danger");
            else
                Toasts.show(qsTr("Theme exported to %1.").arg(path), "success");
        }
    }

    FileDialog {
        id: importDialog

        title: qsTr("Import a terminal theme")
        fileMode: FileDialog.OpenFile
        nameFilters: [qsTr("Terminal themes (%1)").arg(TerminalProfiles.choices("import_patterns").join(" ")), qsTr("All files (*)")]
        onAccepted: TerminalProfiles.importTheme(selectedFile)
    }

    FileDialog {
        id: exportDialog

        title: qsTr("Export the theme")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "toml"
        nameFilters: [qsTr("OpenSesh theme (*.toml)"), qsTr("Alacritty colors (*.toml)")]
        onAccepted: TerminalProfiles.exportTheme(page.selectedId, selectedNameFilter.index === 1 ? "alacritty" : "opensesh", selectedFile)
    }

    ThemeEditorDialog {
        id: editor
    }
}
