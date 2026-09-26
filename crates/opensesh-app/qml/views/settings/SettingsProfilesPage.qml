// Settings > Profiles (PLAN §6.2): the terminal profiles. The default profile applies to every
// terminal; other profiles only change what they set, and groups, hosts (Sprint 5) and tabs can
// pick one. Create, copy, rename and delete profiles, choose the one new tabs use, and open one
// in Settings > Terminal to edit it.
pragma ComponentBehavior: Bound

import QtQuick
import cc.caixa.opensesh

SettingsPage {
    id: page

    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")
    property string selectedId: AppSettings.terminalProfile

    readonly property var selected: {
        for (const profile of profileList) {
            if (profile.id === selectedId)
                return profile;
        }
        return profileList.length > 0 ? profileList[0] : null;
    }

    // Opens `id` in Settings > Terminal (after this handler: the switch replaces the page).
    function editProfile(id) {
        let view = page.parent;
        while (view && typeof view.showSection !== "function")
            view = view.parent;
        if (!view)
            return;
        Qt.callLater(() => {
            view.showSection("terminal");
            const loaded = view.currentPage ? view.currentPage() : null;
            if (loaded && loaded.profileId !== undefined)
                loaded.profileId = id;
        });
    }

    title: qsTr("Profiles")
    description: qsTr("A profile is a set of terminal settings. The default profile applies everywhere; other profiles only change what they set, and tabs can switch profile from their menu.")

    SettingsGroup {
        width: parent.width
        title: qsTr("Your profiles")

        Column {
            width: parent.width
            spacing: Theme.spacingXs

            Repeater {
                model: page.profileList

                delegate: OsListRow {
                    id: profileRow

                    required property var modelData

                    width: parent.width
                    iconName: modelData.isDefault ? "star" : "user"
                    text: modelData.name
                    subtitle: modelData.isDefault ? qsTr("Applies to every terminal") : qsTr("Changes only what it sets")
                    trailingText: modelData.id === AppSettings.terminalProfile ? qsTr("New tabs") : ""
                    selected: page.selected !== null && page.selected.id === modelData.id
                    Accessible.name: text

                    onClicked: page.selectedId = modelData.id
                }
            }
        }

        Flow {
            width: parent.width
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Edit in Terminal settings")
                iconName: "pencil"
                variant: "primary"
                enabled: page.selected !== null
                onClicked: page.editProfile(page.selected.id)
            }

            OsButton {
                text: qsTr("Use for new tabs")
                iconName: "square-terminal"
                enabled: page.selected !== null && page.selected.id !== AppSettings.terminalProfile
                onClicked: AppSettings.terminalProfile = page.selected.id
            }

            OsButton {
                text: qsTr("New profile")
                iconName: "plus"
                onClicked: nameDialog.ask("create", "", qsTr("New profile"))
            }

            OsButton {
                text: qsTr("Copy")
                iconName: "copy"
                enabled: page.selected !== null && !page.selected.isDefault
                onClicked: nameDialog.ask("copy", page.selected.id, qsTr("%1 (copy)").arg(page.selected.name))
            }

            OsButton {
                text: qsTr("Rename")
                enabled: page.selected !== null && !page.selected.readOnly
                onClicked: nameDialog.ask("rename", page.selected.id, page.selected.name)
            }

            OsButton {
                text: qsTr("Delete")
                iconName: "trash-2"
                variant: "danger"
                enabled: page.selected !== null && !page.selected.isDefault
                onClicked: deleteDialog.open()
            }
        }
    }

    SettingsGroup {
        width: parent.width
        title: qsTr("Files")

        OsText {
            width: parent.width
            text: qsTr("Profiles are TOML files in the profiles folder of %1. You can edit them in any editor; changes apply at once.").arg(TerminalProfiles.folder)
            muted: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            horizontalAlignment: Text.AlignLeft
        }

        Repeater {
            model: TerminalProfiles.problems

            delegate: OsText {
                required property string modelData

                width: parent.width
                text: modelData
                size: "small"
                color: Theme.warning
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }

    OsDialog {
        id: nameDialog

        property string mode
        property string sourceId

        function ask(mode, sourceId, name) {
            nameDialog.mode = mode;
            nameDialog.sourceId = sourceId;
            nameField.text = name;
            open();
            nameField.forceActiveFocus(Qt.OtherFocusReason);
            nameField.selectAll();
        }

        title: mode === "rename" ? qsTr("Rename profile") : mode === "copy" ? qsTr("Copy profile") : qsTr("New profile")
        acceptText: mode === "rename" ? qsTr("Rename") : qsTr("Create")
        acceptEnabled: nameField.text.trim().length > 0

        onAccepted: {
            const name = nameField.text.trim();
            if (mode === "rename") {
                TerminalProfiles.renameProfile(sourceId, name);
                return;
            }
            const id = TerminalProfiles.createProfile(name, mode === "copy" ? sourceId : "");
            if (id.length > 0)
                page.selectedId = id;
        }

        Column {
            width: Math.min(Theme.spacingXxl * 12, nameDialog.maxWidth - nameDialog.leftPadding - nameDialog.rightPadding)
            spacing: Theme.spacingSm

            OsTextField {
                id: nameField

                width: parent.width
                placeholderText: qsTr("Profile name")
                onAccepted: {
                    if (nameDialog.acceptEnabled)
                        nameDialog.accept();
                }
            }

            OsText {
                width: parent.width
                visible: nameDialog.mode === "create"
                text: qsTr("A new profile starts with everything inherited from the default profile.")
                muted: true
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }

    OsDialog {
        id: deleteDialog

        title: qsTr("Delete this profile?")
        acceptText: qsTr("Delete")
        dangerous: true

        onAccepted: {
            if (page.selected && TerminalProfiles.deleteProfile(page.selected.id)) {
                if (AppSettings.terminalProfile === page.selectedId)
                    AppSettings.terminalProfile = "default";
                page.selectedId = "default";
            }
        }

        Column {
            width: Math.min(Theme.spacingXxl * 12, deleteDialog.maxWidth - deleteDialog.leftPadding - deleteDialog.rightPadding)

            OsText {
                width: parent.width
                text: qsTr("Tabs that use %1 go back to the default profile.").arg(page.selected ? page.selected.name : "")
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                horizontalAlignment: Text.AlignLeft
            }
        }
    }
}
