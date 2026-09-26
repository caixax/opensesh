pragma ComponentBehavior: Bound

// Group editor (PLAN Sprint 5): name, parent group, color, notes, and the defaults its hosts and
// subgroups inherit (user, port, jump hosts, key file, terminal profile, SSH and SFTP options).
// Each default shows what the group inherits from its own parents when it sets nothing.
// Functions: edit(id), create(parent) (a new group inside `parent`, "" for the top).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property var draft: ({})
    property var inheritedMap: ({})
    property var errors: ({})
    property bool isNew: true
    readonly property bool readOnly: false
    property int revision: 0
    readonly property var groupList: JSON.parse(Hosts.groups || "[]")
    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")
    readonly property var onOff: [{ text: qsTr("On"), value: true }, { text: qsTr("Off"), value: false }]

    signal loaded

    function edit(id) {
        const group = JSON.parse(Hosts.groupJson(id) || "{}");
        if (!group.id)
            return;
        delete group.inherited;
        delete group.path;
        isNew = false;
        open_(group);
    }

    function create(parent) {
        isNew = true;
        const group = { id: "", name: "" };
        if (parent && parent.length > 0)
            group.parent = parent;
        open_(group);
    }

    function open_(group) {
        draft = group;
        refreshInherited();
        validate();
        revision += 1;
        open();
        loaded();
        Qt.callLater(() => nameRow.focusField());
    }

    function value(path) {
        let node = draft;
        for (const part of path.split(".")) {
            if (node === undefined || node === null)
                return undefined;
            node = node[part];
        }
        return node;
    }

    function setValue(path, next) {
        const parts = path.split(".");
        const copy = JSON.parse(JSON.stringify(draft));
        let node = copy;
        for (let i = 0; i < parts.length - 1; ++i) {
            if (typeof node[parts[i]] !== "object" || node[parts[i]] === null)
                node[parts[i]] = {};
            node = node[parts[i]];
        }
        const last = parts[parts.length - 1];
        if (next === undefined || next === null || next === "")
            delete node[last];
        else
            node[last] = next;
        draft = copy;
        if (path === "parent")
            refreshInherited();
        validate();
        revision += 1;
    }

    function refreshInherited() {
        inheritedMap = JSON.parse(Hosts.inherited(draft.parent ?? "", "ssh") || "{}");
    }

    function validate() {
        errors = JSON.parse(Hosts.validateGroup(JSON.stringify(draft)) || "{}");
    }

    function inheritedInfo(key) {
        return inheritedMap[key] ?? null;
    }

    function inheritedText(key) {
        const info = inheritedMap[key];
        if (!info || info.value === null || info.value === undefined || info.value === "")
            return "";
        let text = String(info.value);
        if (info.value === true)
            text = qsTr("On");
        else if (info.value === false)
            text = qsTr("Off");
        else if (Array.isArray(info.value))
            text = info.value.length > 0 ? info.value.join(", ") : qsTr("none");
        return info.origin === "group" ? qsTr("%1 (from %2)").arg(text).arg(info.groupName) : qsTr("%1 (default)").arg(text);
    }

    function errorText(field) {
        const code = errors[field];
        if (!code)
            return "";
        if (code === "required")
            return qsTr("Required.");
        switch (field) {
        case "parent":
            return qsTr("A group can't go inside itself or one of its subgroups.");
        case "user":
            return qsTr("Not a user name (no spaces, and not starting with -).");
        case "port":
            return qsTr("Use a port from 1 to 65535.");
        case "jump":
            return qsTr("Each jump host is a saved host or user@host:port, separated by commas.");
        default:
            return qsTr("This group can't be saved as it is.");
        }
    }

    title: isNew ? qsTr("New group") : qsTr("Edit group")
    acceptText: qsTr("Save")
    acceptEnabled: revision >= 0 && Object.keys(errors).length === 0

    onAccepted: {
        if (Hosts.saveGroup(JSON.stringify(draft)).length === 0)
            Toasts.show(qsTr("The group could not be saved."), "danger");
    }

    Column {
        width: Math.min(Theme.spacingXxl * 15, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)

        Flickable {
            id: flick

            width: parent.width
            height: Math.min(form.implicitHeight, Math.max(Theme.spacingXxl * 5, dialog.maxHeight - Theme.spacingXxl * 4))
            clip: true
            contentWidth: width
            contentHeight: form.implicitHeight
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: form

                width: flick.width
                spacing: Theme.spacingMd

                EditorTextRow {
                    id: nameRow

                    editor: dialog
                    path: "name"
                    inheritKey: ""
                    label: qsTr("Name")
                    placeholder: qsTr("Production")
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "parent"
                    inherit: false
                    label: qsTr("Inside")
                    options: [{ text: qsTr("No group (top level)"), value: undefined }].concat(dialog.groupList.filter(group => group.id !== dialog.draft.id).map(group => ({
                        text: group.path,
                        value: group.id
                    })))
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "color"
                    inherit: false
                    label: qsTr("Color")
                    options: [{ text: qsTr("None"), value: undefined }].concat(TabColors.options)
                }

                OsSectionHeader {
                    width: parent.width
                    title: qsTr("Defaults for its hosts")
                    description: qsTr("Hosts and subgroups use these unless they set their own.")
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.user"
                    inheritKey: "user"
                    field: "user"
                    label: qsTr("User")
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.port"
                    inheritKey: "port"
                    field: "port"
                    type: "int"
                    label: qsTr("Port")
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.jump"
                    inheritKey: "jump"
                    field: "jump"
                    type: "list"
                    label: qsTr("Jump hosts")
                    placeholder: qsTr("bastion")
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.identity_file"
                    inheritKey: "identity_file"
                    label: qsTr("Private key file")
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "defaults.profile"
                    inheritKey: "profile"
                    label: qsTr("Terminal profile")
                    options: dialog.profileList.map(profile => ({ text: profile.name, value: profile.id }))
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.ssh.keepalive_secs"
                    inheritKey: "ssh.keepalive_secs"
                    type: "int"
                    label: qsTr("Keepalive (seconds)")
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "defaults.ssh.compression"
                    inheritKey: "ssh.compression"
                    label: qsTr("Compression")
                    options: dialog.onOff
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "defaults.ssh.agent_forwarding"
                    inheritKey: "ssh.agent_forwarding"
                    label: qsTr("Agent forwarding")
                    options: dialog.onOff
                    helpText: dialog.revision >= 0 && dialog.value("defaults.ssh.agent_forwarding") === true
                              ? qsTr("Anyone with root on these hosts can use your keys while you are connected.") : ""
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "defaults.ssh.x11"
                    inheritKey: "ssh.x11"
                    label: qsTr("X11 forwarding")
                    options: [{ text: qsTr("Off"), value: "off" }, { text: qsTr("Untrusted"), value: "untrusted" },
                        { text: qsTr("Trusted"), value: "trusted" }]
                }

                EditorChoiceRow {
                    editor: dialog
                    path: "defaults.sftp.follow_cwd"
                    inheritKey: "sftp.follow_cwd"
                    label: qsTr("SFTP follows the terminal")
                    options: dialog.onOff
                }

                EditorTextRow {
                    editor: dialog
                    path: "defaults.sftp.start_dir"
                    inheritKey: "sftp.start_dir"
                    label: qsTr("SFTP start folder")
                }

                EditorTextRow {
                    editor: dialog
                    path: "notes"
                    inheritKey: ""
                    label: qsTr("Notes")
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }
    }
}
