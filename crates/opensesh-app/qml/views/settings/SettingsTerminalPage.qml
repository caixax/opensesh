// Settings > Terminal (PLAN §6.2): every terminal option of one profile, with a live preview of
// the sample (ls --color, git diff, a powerline prompt, CJK and emoji) drawn by the real terminal
// engine and renderer. Changes are stored in the profile at once and reach open terminals live.
// In a profile other than the default one, each option shows whether it is the profile's own or
// inherited, and can be made to inherit again.
//   profileId: string   the profile being edited (starts with the one new tabs use)
pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Dialogs
import cc.caixa.opensesh

SettingsPage {
    id: page

    property string profileId: AppSettings.terminalProfile

    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")
    readonly property var themeList: JSON.parse(TerminalProfiles.themes || "[]")
    readonly property var setList: JSON.parse(TerminalProfiles.highlightSets || "[]")
    readonly property var profileData: TerminalProfiles.revision >= 0 ? JSON.parse(TerminalProfiles.profileJson(profileId) || "{}") : ({})
    readonly property var values: profileData.values || ({})
    readonly property var setKeys: profileData.set || []
    readonly property bool isDefault: profileData.isDefault === true
    readonly property bool readOnly: profileData.readOnly === true
    // The theme in use right now (dark or light), for the color options' starting values.
    readonly property var currentTheme: themeById(Theme.dark ? values.theme_dark : values.theme_light)
    readonly property var themeSwatches: currentTheme ? currentTheme.colors.normal.concat(currentTheme.colors.bright) : []
    readonly property bool linux: Qt.platform.os === "linux"

    readonly property var weightNames: ({
            "100": qsTr("Thin"),
            "200": qsTr("Extra light"),
            "300": qsTr("Light"),
            "400": qsTr("Regular"),
            "500": qsTr("Medium"),
            "600": qsTr("Semibold"),
            "700": qsTr("Bold"),
            "800": qsTr("Extra bold"),
            "900": qsTr("Black")
        })

    function themeById(id) {
        for (const theme of themeList) {
            if (theme.id === id)
                return theme;
        }
        return null;
    }

    function set(key, value) {
        const error = TerminalProfiles.setOption(profileId, key, JSON.stringify(value));
        if (error.length > 0)
            Toasts.show(qsTr("Could not change this setting: %1").arg(error), "warning");
    }

    function reset(key) {
        TerminalProfiles.resetOption(profileId, key);
    }

    function weightModel() {
        const out = [];
        for (let weight = 100; weight <= 900; weight += 100)
            out.push({
                value: weight,
                text: qsTr("%1 (%2)").arg(weightNames[String(weight)]).arg(weight)
            });
        return out;
    }

    // Shows another settings section; the switch happens after this page's handler returns,
    // since it replaces the page.
    function showSection(id) {
        let view = page.parent;
        while (view && typeof view.showSection !== "function")
            view = view.parent;
        if (view)
            Qt.callLater(() => view.showSection(id));
    }

    // For the smoke test.
    function openRulesDialog(id) {
        rulesDialog.openSet(id);
    }

    function closeRulesDialog() {
        rulesDialog.close();
    }

    function toggleSet(id, on) {
        const current = (values.highlight_sets || []).filter(other => other !== id);
        if (on)
            current.push(id);
        set("highlight_sets", current);
    }

    title: qsTr("Terminal")
    description: qsTr("Font, colors, cursor, scrollback, clipboard and behavior of the terminal. Settings belong to a profile: the default profile applies everywhere, and other profiles change only what they set.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Profile")

        SettingsRow {
            label: qsTr("Editing")
            helpText: page.readOnly ? qsTr("This profile's file comes from a newer OpenSesh or can't be read, so it can't be changed here.")
                                    : qsTr("New terminal tabs use the profile chosen in Settings > Profiles.")

            Row {
                width: parent.width
                spacing: Theme.spacingSm

                OsComboBox {
                    id: profileBox

                    width: parent.width - manageButton.width - parent.spacing
                    textRole: "name"
                    valueRole: "id"
                    model: page.profileList
                    currentIndex: {
                        for (let i = 0; i < page.profileList.length; ++i) {
                            if (page.profileList[i].id === page.profileId)
                                return i;
                        }
                        return 0;
                    }
                    Accessible.name: qsTr("Profile to edit")
                    onActivated: index => page.profileId = page.profileList[index].id
                }

                OsButton {
                    id: manageButton

                    text: qsTr("Profiles…")
                    iconName: "user"
                    onClicked: page.showSection("profiles")
                }
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Preview")

        Rectangle {
            width: parent.width
            height: preview.cellHeight > 0 ? preview.cellHeight * 16 + 2 * preview.padding : Theme.spacingXxl * 8
            radius: Theme.radiusControl
            color: preview.backgroundColor.length > 0 ? preview.backgroundColor : Theme.bg
            border.width: Theme.borderWidth
            border.color: Theme.border
            clip: true

            TerminalItem {
                id: preview

                anchors.fill: parent
                anchors.margins: Theme.borderWidth
                preview: true
                dark: Theme.dark
                profileId: page.profileId
                settingsRevision: TerminalProfiles.revision
                reduceMotion: Theme.reduceMotion
                Accessible.role: Accessible.Terminal
                Accessible.name: qsTr("Terminal preview")
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Font")

        TerminalOptionRow {
            page: page
            key: "font_family"
            label: qsTr("Font")
            note: qsTr("Monospaced fonts are listed unless you show all fonts. Empty uses the bundled JetBrains Mono.")

            Column {
                width: parent.width
                spacing: Theme.spacingSm

                OsFontPicker {
                    width: parent.width
                    monospaceOnly: !allFonts.checked
                    Accessible.name: qsTr("Font")
                    onActivated: page.set("font_family", currentFamily)

                    // The picker assigns `currentFamily` itself when the user picks: keep it tied
                    // to the profile (another profile, an edit on disk) with a Binding element.
                    Binding on currentFamily {
                        value: page.values.font_family && page.values.font_family.length > 0 ? page.values.font_family : "JetBrains Mono"
                        restoreMode: Binding.RestoreNone
                    }
                }

                OsSwitch {
                    id: allFonts

                    text: qsTr("Show all fonts")
                }
            }
        }

        TerminalOptionRow {
            page: page
            key: "font_fallbacks"
            label: qsTr("Fallback fonts")
            note: qsTr("Tried in order for characters the font lacks, such as a Nerd Font for icons or a CJK font. Separate them with commas.")

            OsTextField {
                width: parent.width
                text: (page.values.font_fallbacks || []).join(", ")
                placeholderText: qsTr("e.g. Symbols Nerd Font Mono, Noto Sans CJK JP")
                Accessible.name: qsTr("Fallback fonts")
                onEditingFinished: page.set("font_fallbacks", text.split(",").map(part => part.trim()).filter(part => part.length > 0))
            }
        }

        TerminalOptionRow {
            page: page
            key: "font_size"
            label: qsTr("Size")
            note: qsTr("Ctrl+= and Ctrl+- change the size of one tab.")

            OsSpinBox {
                from: 4
                to: 96
                value: Math.round(page.values.font_size || 11)
                Accessible.name: qsTr("Font size in points")
                onValueModified: page.set("font_size", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "font_weight"
            label: qsTr("Weight")

            OsComboBox {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                textRole: "text"
                valueRole: "value"
                model: page.weightModel()
                currentIndex: Math.round((page.values.font_weight || 400) / 100) - 1
                Accessible.name: qsTr("Weight of normal text")
                onActivated: index => page.set("font_weight", (index + 1) * 100)
            }
        }

        TerminalOptionRow {
            page: page
            key: "font_weight_bold"
            label: qsTr("Bold weight")

            OsComboBox {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                textRole: "text"
                valueRole: "value"
                model: page.weightModel()
                currentIndex: Math.round((page.values.font_weight_bold || 700) / 100) - 1
                Accessible.name: qsTr("Weight of bold text")
                onActivated: index => page.set("font_weight_bold", (index + 1) * 100)
            }
        }

        TerminalOptionRow {
            page: page
            key: "font_italic"
            label: qsTr("Italic")

            OsSwitch {
                text: qsTr("Draw italic text in italics")
                checked: page.values.font_italic !== false
                onToggled: page.set("font_italic", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "line_height"
            label: qsTr("Line height")

            SliderValue {
                width: parent.width
                from: 0.8
                to: 2.0
                stepSize: 0.05
                value: page.values.line_height || 1.0
                text: qsTr("%1%").arg(Math.round(sliderValue * 100))
                Accessible.name: qsTr("Line height")
                onCommitted: value => page.set("line_height", Math.round(value * 100) / 100)
            }
        }

        TerminalOptionRow {
            page: page
            key: "letter_spacing"
            label: qsTr("Letter spacing")

            SliderValue {
                width: parent.width
                from: -2
                to: 10
                stepSize: 0.5
                value: page.values.letter_spacing || 0
                text: qsTr("%1 px").arg(sliderValue.toFixed(1))
                Accessible.name: qsTr("Letter spacing in pixels")
                onCommitted: value => page.set("letter_spacing", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "antialiasing"
            label: qsTr("Smoothing")

            OsSwitch {
                text: qsTr("Smooth the edges of letters (antialiasing)")
                checked: page.values.antialiasing !== false
                onToggled: page.set("antialiasing", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "hinting"
            label: qsTr("Hinting")
            note: qsTr("How much letters are fitted to the pixel grid.")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("hinting")
                labels: ({
                        default: qsTr("System"),
                        none: qsTr("None"),
                        vertical: qsTr("Vertical"),
                        full: qsTr("Full")
                    })
                value: page.values.hinting || "default"
                Accessible.name: qsTr("Hinting")
                onPicked: value => page.set("hinting", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "ligatures"
            label: qsTr("Ligatures")
            note: qsTr("Experimental. Joins character pairs such as -> and != in fonts that have them.")

            OsSwitch {
                text: qsTr("Use programming ligatures")
                checked: page.values.ligatures === true
                onToggled: page.set("ligatures", checked)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Colors")

        TerminalOptionRow {
            page: page
            key: "theme_dark"
            label: qsTr("Dark theme")
            note: qsTr("Used while OpenSesh is dark.")

            ThemeBox {
                width: parent.width
                themes: page.themeList
                currentId: page.values.theme_dark || ""
                Accessible.name: qsTr("Dark theme")
                onPicked: id => page.set("theme_dark", id)
            }
        }

        TerminalOptionRow {
            page: page
            key: "theme_light"
            label: qsTr("Light theme")
            note: qsTr("Used while OpenSesh is light.")

            ThemeBox {
                width: parent.width
                themes: page.themeList
                currentId: page.values.theme_light || ""
                Accessible.name: qsTr("Light theme")
                onPicked: id => page.set("theme_light", id)
            }
        }

        TerminalOptionRow {
            page: page
            key: "bold_is_bright"
            label: qsTr("Bold text")

            OsSwitch {
                text: qsTr("Show bold text in bright colors")
                checked: page.values.bold_is_bright !== false
                onToggled: page.set("bold_is_bright", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "minimum_contrast"
            label: qsTr("Minimum contrast")
            note: qsTr("Text that is hard to read on its background is made lighter or darker. 1 turns this off; 4.5 is the WCAG level for normal text.")

            SliderValue {
                width: parent.width
                from: 1
                to: 7
                stepSize: 0.5
                value: page.values.minimum_contrast || 1
                text: sliderValue <= 1 ? qsTr("Off") : qsTr("%1:1").arg(sliderValue.toFixed(1))
                Accessible.name: qsTr("Minimum contrast")
                onCommitted: value => page.set("minimum_contrast", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "cursor_color"
            label: qsTr("Cursor color")

            ThemeColorOption {
                width: parent.width
                page: page
                key: "cursor_color"
                label: qsTr("Cursor color")
                themeColor: page.currentTheme ? page.currentTheme.colors.cursor : ""
            }
        }

        TerminalOptionRow {
            page: page
            key: "cursor_text_color"
            label: qsTr("Text under the cursor")

            ThemeColorOption {
                width: parent.width
                page: page
                key: "cursor_text_color"
                label: qsTr("Color of the text under the cursor")
                themeColor: page.currentTheme ? page.currentTheme.colors.cursorText : ""
            }
        }

        TerminalOptionRow {
            page: page
            key: "selection_background"
            label: qsTr("Selection")

            ThemeColorOption {
                width: parent.width
                page: page
                key: "selection_background"
                label: qsTr("Selection color")
                themeColor: page.currentTheme ? page.currentTheme.colors.selectionBackground : ""
            }
        }

        TerminalOptionRow {
            page: page
            key: "selection_foreground"
            label: qsTr("Selected text")
            note: qsTr("The theme's choice may keep each character's own color.")

            ThemeColorOption {
                width: parent.width
                page: page
                key: "selection_foreground"
                label: qsTr("Color of selected text")
                themeColor: page.currentTheme ? (page.currentTheme.colors.selectionForeground || page.currentTheme.colors.foreground) : ""
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Cursor")

        TerminalOptionRow {
            page: page
            key: "cursor_shape"
            label: qsTr("Shape")
            note: qsTr("Programs can choose their own shape.")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("cursor_shape")
                labels: ({
                        block: qsTr("Block"),
                        beam: qsTr("Bar"),
                        underline: qsTr("Underline")
                    })
                value: page.values.cursor_shape || "block"
                Accessible.name: qsTr("Cursor shape")
                onPicked: value => page.set("cursor_shape", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "cursor_blinking"
            label: qsTr("Blinking")
            note: qsTr("Reduce motion in Settings > Appearance stops all blinking.")

            OsSwitch {
                text: qsTr("Blink the cursor")
                checked: page.values.cursor_blinking === true
                onToggled: page.set("cursor_blinking", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "cursor_hollow_unfocused"
            label: qsTr("Without focus")

            OsSwitch {
                text: qsTr("Show a hollow block when the terminal isn't focused")
                checked: page.values.cursor_hollow_unfocused !== false
                onToggled: page.set("cursor_hollow_unfocused", checked)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Window")

        TerminalOptionRow {
            page: page
            key: "padding"
            label: qsTr("Padding")
            note: qsTr("Space around the text, in pixels.")

            OsSpinBox {
                from: 0
                to: 64
                value: page.values.padding !== undefined ? page.values.padding : 8
                Accessible.name: qsTr("Padding in pixels")
                onValueModified: page.set("padding", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "background_opacity"
            label: qsTr("Background opacity")
            note: AppInfo.windowAlpha ? qsTr("Below 100% the terminal shows what is behind the window. Blur, if any, comes from your desktop.")
                                      : qsTr("Below 100% the terminal shows what is behind the window after OpenSesh restarts (the window needs a transparent background). Blur, if any, comes from your desktop.")

            SliderValue {
                width: parent.width
                from: 0.2
                to: 1
                stepSize: 0.05
                value: page.values.background_opacity !== undefined ? page.values.background_opacity : 1
                text: qsTr("%1%").arg(Math.round(sliderValue * 100))
                Accessible.name: qsTr("Background opacity")
                onCommitted: value => page.set("background_opacity", Math.round(value * 100) / 100)
            }
        }

        TerminalOptionRow {
            page: page
            key: "background_image"
            label: qsTr("Background image")

            Row {
                width: parent.width
                spacing: Theme.spacingSm

                OsTextField {
                    id: imageField

                    width: parent.width - browseImage.width - clearImage.width - 2 * parent.spacing
                    text: page.values.background_image || ""
                    placeholderText: qsTr("No image")
                    Accessible.name: qsTr("Background image file")
                    onEditingFinished: page.set("background_image", text.trim())
                }

                OsButton {
                    id: browseImage

                    text: qsTr("Choose…")
                    onClicked: imageDialog.open()
                }

                OsIconButton {
                    id: clearImage

                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "x"
                    toolTip: qsTr("Remove the image")
                    enabled: imageField.text.length > 0
                    onClicked: page.set("background_image", "")
                }
            }
        }

        TerminalOptionRow {
            page: page
            key: "background_image_dim"
            label: qsTr("Image dimming")
            note: qsTr("How much the theme's background covers the image, so text stays readable.")

            SliderValue {
                width: parent.width
                from: 0
                to: 1
                stepSize: 0.05
                value: page.values.background_image_dim !== undefined ? page.values.background_image_dim : 0.8
                text: qsTr("%1%").arg(Math.round(sliderValue * 100))
                Accessible.name: qsTr("Image dimming")
                onCommitted: value => page.set("background_image_dim", Math.round(value * 100) / 100)
            }
        }

        TerminalOptionRow {
            page: page
            key: "background_image_fit"
            label: qsTr("Image fit")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("background_image_fit")
                labels: ({
                        cover: qsTr("Fill"),
                        contain: qsTr("Fit"),
                        stretch: qsTr("Stretch"),
                        tile: qsTr("Tile"),
                        center: qsTr("Center")
                    })
                value: page.values.background_image_fit || "cover"
                Accessible.name: qsTr("Image fit")
                onPicked: value => page.set("background_image_fit", value)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Scrolling")

        TerminalOptionRow {
            page: page
            key: "scrollback_lines"
            label: qsTr("Scrollback")
            note: qsTr("Lines of history kept for each terminal (up to 100,000).")

            OsSpinBox {
                from: 0
                to: 100000
                stepSize: 1000
                value: page.values.scrollback_lines !== undefined ? page.values.scrollback_lines : 10000
                Accessible.name: qsTr("Scrollback lines")
                onValueModified: page.set("scrollback_lines", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "scroll_speed"
            label: qsTr("Scroll speed")

            SliderValue {
                width: parent.width
                from: 0.25
                to: 5
                stepSize: 0.25
                value: page.values.scroll_speed || 1
                text: qsTr("%1×").arg(sliderValue.toFixed(2))
                Accessible.name: qsTr("Scroll speed")
                onCommitted: value => page.set("scroll_speed", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "smooth_scroll"
            label: qsTr("Smooth scrolling")

            OsSwitch {
                text: qsTr("Animate wheel scrolling")
                checked: page.values.smooth_scroll === true
                onToggled: page.set("smooth_scroll", checked)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Selection and clipboard")

        TerminalOptionRow {
            page: page
            key: "word_separators"
            label: qsTr("Word separators")
            note: qsTr("Characters that end a word when you double-click.")

            OsTextField {
                width: parent.width
                text: page.values.word_separators || ""
                font.family: Theme.monoFontFamily
                Accessible.name: qsTr("Word separators")
                onEditingFinished: page.set("word_separators", text)
            }
        }

        TerminalOptionRow {
            page: page
            key: "copy_on_select"
            label: qsTr("Copy on select")

            OsSwitch {
                text: qsTr("Copy selected text to the clipboard at once")
                checked: page.values.copy_on_select === true
                onToggled: page.set("copy_on_select", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "right_click"
            label: qsTr("Right click")
            note: qsTr("Shift+right click always opens the menu.")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("right_click")
                labels: ({
                        menu: qsTr("Opens the menu"),
                        paste: qsTr("Pastes")
                    })
                value: page.values.right_click || "menu"
                Accessible.name: qsTr("Right click")
                onPicked: value => page.set("right_click", value)
            }
        }

        TerminalOptionRow {
            visible: page.linux
            page: page
            key: "primary_selection"
            label: qsTr("Primary selection")

            OsSwitch {
                text: qsTr("Selecting text sets the primary selection; the middle button pastes it")
                checked: page.values.primary_selection !== false
                onToggled: page.set("primary_selection", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "osc52"
            label: qsTr("Clipboard access")
            note: qsTr("Lets programs, even on remote servers, put text on your clipboard (OSC 52). They can never read it.")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("osc52")
                labels: ({
                        off: qsTr("Off"),
                        copy: qsTr("Programs may copy")
                    })
                value: page.values.osc52 || "off"
                Accessible.name: qsTr("Clipboard access for programs")
                onPicked: value => page.set("osc52", value)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Behavior")

        TerminalOptionRow {
            page: page
            key: "bell"
            label: qsTr("Bell")
            note: qsTr("Notification flashes the taskbar and adds a notice when OpenSesh isn't the active window. The sound is not available on Wayland; the bell flashes instead.")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("bell")
                labels: ({
                        visual: qsTr("Flash"),
                        sound: qsTr("Sound"),
                        notification: qsTr("Notification"),
                        none: qsTr("None")
                    })
                value: page.values.bell || "visual"
                Accessible.name: qsTr("Bell")
                onPicked: value => page.set("bell", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "term"
            label: qsTr("Terminal type")
            note: qsTr("The TERM variable of new terminals.")

            OsTextField {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                text: page.values.term || ""
                font.family: Theme.monoFontFamily
                Accessible.name: qsTr("Terminal type")
                onEditingFinished: page.set("term", text.trim())
            }
        }

        TerminalOptionRow {
            page: page
            key: "backspace"
            label: qsTr("Backspace sends")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("backspace")
                labels: ({
                        del: qsTr("DEL (^?)"),
                        ctrl_h: qsTr("BS (^H)")
                    })
                value: page.values.backspace || "del"
                Accessible.name: qsTr("What Backspace sends")
                onPicked: value => page.set("backspace", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "delete"
            label: qsTr("Delete sends")

            SettingsChoice {
                width: parent.width
                values: TerminalProfiles.choices("delete")
                labels: ({
                        vt220: qsTr("VT220 (ESC [3~)"),
                        del: qsTr("DEL (^?)")
                    })
                value: page.values["delete"] || "vt220"
                Accessible.name: qsTr("What Delete sends")
                onPicked: value => page.set("delete", value)
            }
        }

        TerminalOptionRow {
            page: page
            key: "alt_as_meta"
            label: qsTr("Alt key")

            OsSwitch {
                text: qsTr("Alt acts as Meta (sends Escape before the key)")
                checked: page.values.alt_as_meta !== false
                onToggled: page.set("alt_as_meta", checked)
            }
        }

        TerminalOptionRow {
            page: page
            key: "encoding"
            label: qsTr("Encoding")
            note: qsTr("For old systems and devices that don't use UTF-8.")

            OsComboBox {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                model: TerminalProfiles.choices("encoding")
                currentIndex: Math.max(0, model.indexOf(page.values.encoding || "UTF-8"))
                Accessible.name: qsTr("Encoding")
                onActivated: index => page.set("encoding", model[index])
            }
        }

        TerminalOptionRow {
            page: page
            key: "answerback"
            label: qsTr("Answerback")
            note: qsTr("Sent when a program asks with Ctrl+E (ENQ). Printable ASCII only; empty sends nothing.")

            OsTextField {
                width: Math.min(parent.width, Theme.spacingXxl * 8)
                text: page.values.answerback || ""
                maximumLength: 64
                Accessible.name: qsTr("Answerback message")
                onEditingFinished: page.set("answerback", text)
            }
        }

        TerminalOptionRow {
            page: page
            key: "paste_line_delay_ms"
            label: qsTr("Paste delay")
            note: qsTr("Pause between the lines of a paste, in milliseconds, for slow devices. Escape or Ctrl+C stops a slow paste.")

            OsSpinBox {
                from: 0
                to: 5000
                stepSize: 10
                value: page.values.paste_line_delay_ms || 0
                Accessible.name: qsTr("Paste delay in milliseconds")
                onValueModified: page.set("paste_line_delay_ms", value)
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Keyword highlighting")
        description: qsTr("Rule sets color words and patterns as they appear, like errors in logs or IP addresses. Turn sets on for this profile; each tab can turn highlighting off from its menu.")

        TerminalOptionRow {
            page: page
            key: "highlight_sets"
            label: qsTr("Rule sets")

            Column {
                width: parent.width
                spacing: Theme.spacingXs

                Repeater {
                    model: page.setList

                    delegate: Row {
                        id: setRow

                        required property var modelData

                        width: parent.width
                        spacing: Theme.spacingSm

                        OsCheckBox {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - editSet.width - parent.spacing
                            text: setRow.modelData.builtin ? qsTr("%1 (built-in)").arg(setRow.modelData.name) : setRow.modelData.name
                            checked: (page.values.highlight_sets || []).indexOf(setRow.modelData.id) >= 0
                            onToggled: page.toggleSet(setRow.modelData.id, checked)
                        }

                        OsButton {
                            id: editSet

                            text: setRow.modelData.builtin ? qsTr("View") : qsTr("Edit")
                            variant: "ghost"
                            onClicked: rulesDialog.openSet(setRow.modelData.id)
                        }
                    }
                }

                OsButton {
                    text: qsTr("New rule set")
                    iconName: "plus"
                    onClicked: rulesDialog.openSet("")
                }
            }
        }
    }

    FileDialog {
        id: imageDialog

        title: qsTr("Choose a background image")
        nameFilters: [qsTr("Images (%1)").arg("*.png *.jpg *.jpeg *.webp *.bmp *.gif"), qsTr("All files (*)")]
        onAccepted: page.set("background_image", Platform.localPath(selectedFile))
    }

    HighlightRulesDialog {
        id: rulesDialog
    }
}
