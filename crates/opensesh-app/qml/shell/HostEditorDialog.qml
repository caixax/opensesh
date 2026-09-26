pragma ComponentBehavior: Bound

// Host editor (PLAN Sprint 5): sections Basic, Authentication, Advanced, Terminal, SFTP and Notes.
// Each field shows what the host inherits when it sets nothing ("deploy (from Production)"),
// and an empty field or "Inherit" goes back to inheriting. Problems show under their field as
// the host is edited (Hosts.validateHost); Save is off until there are none. A host linked from
// ~/.ssh/config is shown read-only, with Duplicate to make an editable copy.
// Functions: edit(id), create(group) (a new host in group `group`, "" for none).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    // The host being edited, as Hosts.hostJson gives it (minus `inherited` and `linked`).
    property var draft: ({})
    property var inheritedMap: ({})
    property var errors: ({})
    property bool readOnly: false
    property bool isNew: true
    // Grows with every change of the draft, so rows re-read what they show.
    property int revision: 0
    property int section: 0
    readonly property var sections: [qsTr("Basic"), qsTr("Authentication"), qsTr("Advanced"), qsTr("Terminal"),
        qsTr("SFTP"), qsTr("Notes")]
    readonly property var groupList: JSON.parse(Hosts.groups || "[]")
    readonly property var profileList: JSON.parse(TerminalProfiles.profiles || "[]")
    readonly property var themeList: JSON.parse(TerminalProfiles.themes || "[]")
    readonly property string protocol: revision >= 0 ? (draft.protocol ?? "ssh") : "ssh"

    signal loaded

    function edit(id) {
        const host = JSON.parse(Hosts.hostJson(id) || "{}");
        if (!host.id)
            return;
        readOnly = host.linked === true;
        delete host.linked;
        delete host.inherited;
        isNew = false;
        open_(host);
    }

    function create(group) {
        readOnly = false;
        isNew = true;
        const host = { id: "", name: "", protocol: "ssh", address: "", tags: [], favorite: false };
        if (group && group.length > 0)
            host.group = group;
        open_(host);
    }

    function open_(host) {
        draft = host;
        section = 0;
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
        if (path === "group" || path === "protocol")
            refreshInherited();
        validate();
        revision += 1;
    }

    function refreshInherited() {
        inheritedMap = JSON.parse(Hosts.inherited(draft.group ?? "", draft.protocol ?? "ssh") || "{}");
    }

    function validate() {
        errors = JSON.parse(Hosts.validateHost(JSON.stringify(draft)) || "{}");
    }

    function inheritedInfo(key) {
        return inheritedMap[key] ?? null;
    }

    function valueText(value) {
        if (value === true)
            return qsTr("On");
        if (value === false)
            return qsTr("Off");
        if (Array.isArray(value))
            return value.length > 0 ? value.join(", ") : qsTr("none");
        return String(value);
    }

    // "deploy (from Production)", "22 (default)", or "" when nothing is inherited.
    function inheritedText(key) {
        const info = inheritedMap[key];
        if (!info || info.value === null || info.value === undefined || info.value === "")
            return "";
        const text = valueText(info.value);
        return info.origin === "group" ? qsTr("%1 (from %2)").arg(text).arg(info.groupName) : qsTr("%1 (default)").arg(text);
    }

    function errorText(field) {
        const code = errors[field];
        if (!code)
            return "";
        if (code === "required")
            return qsTr("Required.");
        switch (field) {
        case "address":
            return protocol === "serial" ? qsTr("Not a device name.") : qsTr("Not a host name or address (no spaces, @ or /, and not starting with -).");
        case "user":
            return qsTr("Not a user name (no spaces, and not starting with -).");
        case "port":
            return qsTr("Use a port from 1 to 65535.");
        case "identity_file":
            return qsTr("A file path, not an option.");
        case "jump":
            return qsTr("Each jump host is a saved host or user@host:port, separated by commas.");
        case "group":
            return qsTr("That group no longer exists.");
        default:
            return qsTr("This host can't be saved as it is.");
        }
    }

    function hasErrors() {
        return Object.keys(errors).length > 0;
    }

    function save() {
        if (readOnly) {
            const copy = Hosts.duplicateHost(draft.id);
            if (copy.length > 0)
                Qt.callLater(() => dialog.edit(copy));
            return;
        }
        if (Hosts.saveHost(JSON.stringify(draft)).length === 0)
            Toasts.show(qsTr("The host could not be saved."), "danger");
    }

    readonly property var protocolOptions: [
        { text: qsTr("SSH"), value: "ssh" },
        { text: qsTr("SFTP"), value: "sftp" },
        { text: qsTr("Telnet"), value: "telnet" },
        { text: qsTr("Serial port"), value: "serial" },
        { text: qsTr("Mosh"), value: "mosh" },
        { text: qsTr("RDP (remote desktop)"), value: "rdp" },
        { text: qsTr("VNC"), value: "vnc" },
        { text: qsTr("Local shell"), value: "local" },
        { text: qsTr("Docker container"), value: "docker" },
        { text: qsTr("Kubernetes pod"), value: "kube" }
    ]
    readonly property var onOff: [{ text: qsTr("On"), value: true }, { text: qsTr("Off"), value: false }]
    readonly property var colorOptions: [{ text: qsTr("None"), value: undefined }].concat(TabColors.options)
    readonly property var iconOptions: [
        { text: qsTr("Automatic"), value: undefined },
        { text: qsTr("Server"), value: "server" },
        { text: qsTr("Terminal"), value: "square-terminal" },
        { text: qsTr("Linux"), value: "os-linux" },
        { text: qsTr("Debian"), value: "os-debian" },
        { text: qsTr("Ubuntu"), value: "os-ubuntu" },
        { text: qsTr("Fedora"), value: "os-fedora" },
        { text: qsTr("Arch Linux"), value: "os-archlinux" },
        { text: qsTr("Red Hat"), value: "os-redhat" },
        { text: qsTr("Rocky Linux"), value: "os-rockylinux" },
        { text: qsTr("AlmaLinux"), value: "os-almalinux" },
        { text: qsTr("Alpine Linux"), value: "os-alpinelinux" },
        { text: qsTr("openSUSE"), value: "os-opensuse" },
        { text: qsTr("NixOS"), value: "os-nixos" },
        { text: qsTr("Raspberry Pi"), value: "os-raspberrypi" },
        { text: qsTr("FreeBSD"), value: "os-freebsd" },
        { text: qsTr("Windows"), value: "os-windows" },
        { text: qsTr("macOS"), value: "os-apple" },
        { text: qsTr("Docker"), value: "os-docker" },
        { text: qsTr("Kubernetes"), value: "os-kubernetes" }
    ]

    title: readOnly ? qsTr("Host from ~/.ssh/config") : isNew ? qsTr("New host") : qsTr("Edit host")
    acceptText: readOnly ? qsTr("Duplicate to edit") : qsTr("Save")
    acceptEnabled: readOnly || (revision >= 0 && !hasErrors())

    onAccepted: save()

    // A fixed size the dialog shrinks to fit the window (OsDialog caps it); the sections scroll.
    Item {
        implicitWidth: Theme.spacingXxl * 19
        implicitHeight: Theme.spacingXxl * 13
        width: parent ? parent.width : implicitWidth
        height: parent ? parent.height : implicitHeight

        RowLayout {
            anchors.fill: parent
            spacing: Theme.spacingLg

            ListView {
                id: sectionList

                Layout.preferredWidth: Theme.spacingXxl * 4
                Layout.fillHeight: true
                model: dialog.sections
                currentIndex: dialog.section
                keyNavigationEnabled: true
                activeFocusOnTab: true
                boundsBehavior: Flickable.StopAtBounds
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Sections")

                onCurrentIndexChanged: dialog.section = currentIndex

                delegate: OsListRow {
                    required property string modelData
                    required property int index

                    width: ListView.view.width
                    text: modelData
                    selected: index === dialog.section
                    focusPolicy: Qt.NoFocus
                    onClicked: dialog.section = index
                }
            }

            Rectangle {
                Layout.fillHeight: true
                implicitWidth: Theme.borderWidth
                color: Theme.border
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: dialog.section

                // Basic
                Section {
                    Column {
                        width: parent.width
                        spacing: Theme.spacingMd

                        SettingsNotice {
                            width: parent.width
                            visible: dialog.readOnly
                            kind: "info"
                            title: qsTr("Read-only")
                            lines: [qsTr("This host comes from ~/.ssh/config, which OpenSesh follows but never changes. Duplicate it to edit a copy.")]
                        }

                        EditorTextRow {
                            id: nameRow

                            editor: dialog
                            path: "name"
                            inheritKey: ""
                            label: qsTr("Name")
                            placeholder: qsTr("web-01")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "protocol"
                            inherit: false
                            label: qsTr("Protocol")
                            options: dialog.protocolOptions
                            helpText: {
                                switch (dialog.protocol) {
                                case "ssh":
                                    return qsTr("Connects with the system's OpenSSH client until the built-in one arrives.");
                                case "local":
                                    return "";
                                case "sftp":
                                    return qsTr("Saved now; the file browser arrives in Sprint 8.");
                                case "rdp":
                                    return qsTr("Saved now; remote desktop arrives in Sprint 13.");
                                case "vnc":
                                    return qsTr("Saved now; VNC arrives in Sprint 14.");
                                default:
                                    return qsTr("Saved now; connecting arrives in Sprint 12.");
                                }
                            }
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "address"
                            inheritKey: ""
                            visible: dialog.protocol !== "local"
                            label: dialog.protocol === "serial" ? qsTr("Device") : dialog.protocol === "docker" ? qsTr("Container")
                                                                                                              : dialog.protocol === "kube" ? qsTr("Pod") : qsTr("Address")
                            placeholder: dialog.protocol === "serial" ? qsTr("/dev/ttyUSB0 or COM3") : qsTr("host name or IP address")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "port"
                            type: "int"
                            visible: ["ssh", "sftp", "telnet", "mosh", "rdp", "vnc"].indexOf(dialog.protocol) >= 0
                            label: qsTr("Port")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "group"
                            inherit: false
                            label: qsTr("Group")
                            options: [{ text: qsTr("No group"), value: undefined }].concat(dialog.groupList.map(group => ({
                                text: group.path,
                                value: group.id
                            })))
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "tags"
                            type: "list"
                            inheritKey: ""
                            label: qsTr("Tags")
                            placeholder: qsTr("web, nginx")
                            helpText: qsTr("Separated by commas.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "color"
                            inherit: false
                            label: qsTr("Color")
                            options: dialog.colorOptions
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "icon"
                            inherit: false
                            label: qsTr("Icon")
                            options: dialog.iconOptions
                        }

                        OsFormRow {
                            width: parent.width
                            label: qsTr("Favorite")

                            OsSwitch {
                                enabled: !dialog.readOnly
                                checked: dialog.revision >= 0 && dialog.draft.favorite === true
                                Accessible.name: qsTr("Favorite")
                                onToggled: dialog.setValue("favorite", checked ? true : undefined)
                            }
                        }
                    }
                }

                // Authentication
                Section {
                    Column {
                        width: parent.width
                        spacing: Theme.spacingMd

                        EditorTextRow {
                            editor: dialog
                            path: "user"
                            label: qsTr("User")
                            placeholder: qsTr("the local user name")
                        }

                        EditorTextRow {
                            id: identityRow

                            editor: dialog
                            path: "identity_file"
                            label: qsTr("Private key file")
                            placeholder: qsTr("the keys OpenSSH tries by itself")
                        }

                        OsFormRow {
                            width: parent.width

                            OsButton {
                                text: qsTr("Choose a key file…")
                                iconName: "folder-open"
                                enabled: !dialog.readOnly
                                onClicked: keyDialog.open()
                            }
                        }

                        Note {
                            text: qsTr("Passwords, passphrases and keys kept in OpenSesh's encrypted vault arrive with the keychain. Nothing secret is written to hosts.toml.")
                        }
                    }
                }

                // Advanced
                Section {
                    Column {
                        width: parent.width
                        spacing: Theme.spacingMd

                        EditorTextRow {
                            editor: dialog
                            path: "jump"
                            type: "list"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "sftp"
                            label: qsTr("Jump hosts")
                            placeholder: qsTr("bastion, ops@hop:2222")
                            helpText: qsTr("Saved hosts or user@host:port, first hop first. \"none\" connects directly even if the group has jump hosts.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.backend"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("SSH client")
                            options: [{ text: qsTr("Built-in"), value: "internal" }, { text: qsTr("OpenSSH"), value: "openssh" }]
                            helpText: qsTr("Until the built-in client arrives, OpenSesh connects with OpenSSH either way.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.keepalive_secs"
                            type: "int"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Keepalive (seconds)")
                            helpText: qsTr("0 turns it off.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.compression"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Compression")
                            options: dialog.onOff
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.agent_forwarding"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Agent forwarding")
                            options: dialog.onOff
                            helpText: dialog.revision >= 0 && dialog.value("ssh.agent_forwarding") === true
                                      ? qsTr("Anyone with root on this host can use your keys while you are connected.") : ""
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.x11"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("X11 forwarding")
                            options: [{ text: qsTr("Off"), value: "off" }, { text: qsTr("Untrusted"), value: "untrusted" },
                                { text: qsTr("Trusted"), value: "trusted" }]
                            helpText: dialog.revision >= 0 && dialog.value("ssh.x11") === "trusted"
                                      ? qsTr("Trusted forwarding gives remote programs full access to your display.") : ""
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.startup_snippet"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Startup snippet")
                            helpText: qsTr("Runs once the shell is ready; snippets arrive in Sprint 10.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "serial.baud"
                            type: "int"
                            inheritKey: ""
                            visible: dialog.protocol === "serial"
                            label: qsTr("Speed (baud)")
                            placeholder: "115200"
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.data_bits"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Data bits")
                            options: [{ text: qsTr("Default (8)"), value: undefined }].concat([8, 7, 6, 5].map(bits => ({
                                text: String(bits),
                                value: bits
                            })))
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.parity"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Parity")
                            options: [{ text: qsTr("Default (none)"), value: undefined }, { text: qsTr("None"), value: "none" },
                                { text: qsTr("Even"), value: "even" }, { text: qsTr("Odd"), value: "odd" }]
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.stop_bits"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Stop bits")
                            options: [{ text: qsTr("Default (1)"), value: undefined }].concat([1, 2].map(bits => ({
                                text: String(bits),
                                value: bits
                            })))
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.flow_control"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Flow control")
                            options: [{ text: qsTr("Default (none)"), value: undefined }, { text: qsTr("None"), value: "none" },
                                { text: qsTr("XON/XOFF"), value: "software" }, { text: qsTr("RTS/CTS"), value: "hardware" }]
                        }

                        OsText {
                            width: parent.width
                            visible: ["ssh", "serial"].indexOf(dialog.protocol) < 0
                            text: qsTr("No advanced options for this protocol yet.")
                            muted: true
                        }
                    }
                }

                // Terminal
                Section {
                    Column {
                        width: parent.width
                        spacing: Theme.spacingMd

                        EditorChoiceRow {
                            editor: dialog
                            path: "profile"
                            label: qsTr("Terminal profile")
                            options: dialog.profileList.map(profile => ({ text: profile.name, value: profile.id }))
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "terminal.font_size"
                            type: "int"
                            inheritKey: ""
                            label: qsTr("Font size")
                            placeholder: qsTr("from the profile")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "terminal.theme_dark"
                            inherit: false
                            label: qsTr("Theme (dark mode)")
                            options: [{ text: qsTr("From the profile"), value: undefined }].concat(dialog.themeList.map(theme => ({
                                text: theme.name,
                                value: theme.id
                            })))
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "terminal.theme_light"
                            inherit: false
                            label: qsTr("Theme (light mode)")
                            options: [{ text: qsTr("From the profile"), value: undefined }].concat(dialog.themeList.map(theme => ({
                                text: theme.name,
                                value: theme.id
                            })))
                        }

                        Note {
                            text: qsTr("Every other terminal option comes from the profile. Hosts can override any of them in [host.terminal] in hosts.toml.")
                        }
                    }
                }

                // SFTP
                Section {
                    Column {
                        width: parent.width
                        spacing: Theme.spacingMd

                        EditorChoiceRow {
                            editor: dialog
                            path: "sftp.follow_cwd"
                            label: qsTr("Follow the terminal's folder")
                            options: dialog.onOff
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "sftp.start_dir"
                            label: qsTr("Start folder")
                        }

                        Note {
                            text: qsTr("The SFTP browser arrives in Sprint 8; these settings are kept for it.")
                        }
                    }
                }

                // Notes
                Item {
                    ColumnLayout {
                        anchors.fill: parent
                        spacing: Theme.spacingSm

                        RowLayout {
                            Layout.fillWidth: true

                            OsText {
                                Layout.fillWidth: true
                                text: qsTr("Markdown")
                                muted: true
                                size: "small"
                            }

                            OsSwitch {
                                id: previewSwitch

                                text: qsTr("Preview")
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            radius: Theme.radiusControl
                            color: Theme.surface2
                            border.width: Theme.borderWidth
                            border.color: notesArea.activeFocus ? Theme.accent : Theme.borderStrong

                            Flickable {
                                id: notesFlick

                                anchors.fill: parent
                                anchors.margins: Theme.spacingSm
                                clip: true
                                contentWidth: width
                                contentHeight: previewSwitch.checked ? notesPreview.implicitHeight : notesArea.implicitHeight
                                boundsBehavior: Flickable.StopAtBounds

                                T.TextArea {
                                    id: notesArea

                                    width: notesFlick.width
                                    visible: !previewSwitch.checked
                                    readOnly: dialog.readOnly
                                    wrapMode: TextEdit.Wrap
                                    color: Theme.text
                                    selectionColor: Theme.selection
                                    selectedTextColor: Theme.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize
                                    placeholderText: qsTr("Anything worth remembering about this host.")
                                    placeholderTextColor: Theme.textMuted
                                    Accessible.name: qsTr("Notes")

                                    onTextChanged: {
                                        if (activeFocus)
                                            dialog.setValue("notes", text);
                                    }

                                    Connections {
                                        target: dialog

                                        function onLoaded() {
                                            notesArea.text = dialog.draft.notes ?? "";
                                        }
                                    }
                                }

                                OsText {
                                    id: notesPreview

                                    width: notesFlick.width
                                    visible: previewSwitch.checked
                                    textFormat: Text.MarkdownText
                                    text: dialog.revision >= 0 ? (dialog.draft.notes ?? "") : ""
                                    wrapMode: Text.Wrap
                                    elide: Text.ElideNone
                                    horizontalAlignment: Text.AlignLeft
                                    onLinkActivated: link => Qt.openUrlExternally(link)
                                }

                                T.ScrollBar.vertical: OsScrollBar {}
                            }
                        }
                    }
                }
            }
        }
    }

    FileDialog {
        id: keyDialog

        title: qsTr("Choose a private key")
        onAccepted: {
            dialog.setValue("identity_file", Platform.localPath(selectedFile));
            dialog.loaded();
        }
    }

    // A muted explanation under the fields.
    component Note: OsText {
        width: parent ? parent.width : implicitWidth
        muted: true
        size: "small"
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        horizontalAlignment: Text.AlignLeft
    }

    // A scrollable section page.
    component Section: Flickable {
        default property alias content: body.data

        clip: true
        contentWidth: width
        contentHeight: body.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        Item {
            id: body

            width: parent.width
            implicitHeight: childrenRect.height
        }

        T.ScrollBar.vertical: OsScrollBar {}
    }
}
