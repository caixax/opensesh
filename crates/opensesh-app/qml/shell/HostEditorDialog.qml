pragma ComponentBehavior: Bound

// Host editor (PLAN Sprint 5): sections Basic, Authentication, Advanced, Terminal, SFTP and Notes.
// Each field shows what the host inherits when it sets nothing ("deploy (from Production)"),
// and an empty field or "Inherit" goes back to inheriting. Problems show under their field as
// the host is edited (Hosts.validateHost); Save is off until there are none. A host linked from
// ~/.ssh/config is shown read-only, with Duplicate to make an editable copy. An S3 host's secret
// key goes into the vault, as the password of the host's identity (a new one named after the
// host when it has none): hosts.toml keeps only the identity's id.
//   shell: Item   the AppShell (unlockVault())
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
    property Item shell: null
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
    readonly property var identityList: JSON.parse(Keychain.identities || "[]")
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
        secretField.text = "";
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
        if (typeof value === "object" && value !== null) {
            const names = Object.keys(value);
            return names.length > 0 ? names.map(name => qsTr("%1=%2").arg(name).arg(value[name])).join("; ") : qsTr("none");
        }
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
            if (protocol === "serial")
                return qsTr("Not a device name.");
            if (protocol === "docker" || protocol === "kube")
                return qsTr("Not a container or pod name (no spaces, and not starting with -).");
            if (protocol === "s3")
                return qsTr("A server address like https://host:port, with no path.");
            return qsTr("Not a host name or address (no spaces, @ or /, and not starting with -).");
        case "container.namespace":
        case "container.pod_container":
            return qsTr("Not a name (no spaces or /, and not starting with -).");
        case "container.context":
            return qsTr("Not a context name (no spaces, and not starting with -).");
        case "container.shell":
            return qsTr("A program and its arguments, with quotes closed.");
        case "s3.region":
            return qsTr("Letters, digits and hyphens, as in eu-west-1.");
        case "rdp.resolution":
            return qsTr("A width and a height from 200 to 8192 pixels, as in 1920x1080.");
        case "rdp.domain":
            return qsTr("Not a domain name (no control characters, and not starting with -).");
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
        case "ssh.auth_order":
            return qsTr("Use publickey, keyboard-interactive and password, separated by commas.");
        case "ssh.proxy":
            return qsTr("Use socks5://host:port or http://host:port.");
        case "ssh.env":
            return qsTr("Names are letters, digits and _, as in NAME=value; OTHER=value.");
        default:
            return qsTr("This host can't be saved as it is.");
        }
    }

    // Scrolls the shown section to the row of `path` (only "ssh.x11" for now: screenshots).
    function reveal(path) {
        const rows = { "ssh.x11": x11Row };
        const row = rows[path];
        if (!row)
            return;
        let flick = row.parent;
        while (flick && flick.contentY === undefined)
            flick = flick.parent;
        if (flick)
            flick.contentY = Math.max(0, Math.min(row.mapToItem(flick.contentItem, 0, 0).y - Theme.spacingLg,
                                                  flick.contentHeight - flick.height));
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
        // Taken out of the field at once: it goes to the vault or nowhere.
        const secret = secretField.text;
        secretField.text = "";
        if (protocol === "s3" && secret.length > 0) {
            saveS3Keys(secret);
            return;
        }
        if (Hosts.saveHost(JSON.stringify(draft)).length === 0)
            Toasts.show(qsTr("The host could not be saved."), "danger");
    }

    // The secret key into the vault (it must be open), then the host with its identity.
    function saveS3Keys(secret) {
        if (Keychain.vaultStatus === "locked" && shell) {
            shell.unlockVault(() => dialog.saveS3Keys(secret));
            return;
        }
        const current = identityList.find(identity => identity.id === draft.identity);
        const identity = current ? {
            id: current.id,
            name: current.name,
            user: draft.user ?? current.user,
            key: current.key ?? "",
            notes: current.notes ?? ""
        } : {
            id: "",
            name: qsTr("S3: %1").arg(draft.name),
            user: draft.user ?? "",
            key: "",
            notes: ""
        };
        KeychainTasks.run(Keychain.saveIdentity(JSON.stringify(identity), "set", secret), (code, detail, value) => {
            if (code.length > 0) {
                Toasts.show(KeychainTasks.message(code, detail), "danger");
                return;
            }
            dialog.setValue("identity", value && String(value).length > 0 ? String(value) : identity.id);
            if (Hosts.saveHost(JSON.stringify(dialog.draft)).length === 0)
                Toasts.show(qsTr("The host could not be saved."), "danger");
        });
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
        { text: qsTr("Kubernetes pod"), value: "kube" },
        { text: qsTr("S3 storage"), value: "s3" }
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
    onRejected: secretField.text = ""

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
                            helpText: dialog.protocol === "sftp" ? qsTr("Opens the server's files in the files view, without a terminal.") : ""
                        }

                        EditorTextRow {
                            id: addressRow

                            editor: dialog
                            path: "address"
                            inheritKey: ""
                            visible: dialog.protocol !== "local"
                            label: {
                                switch (dialog.protocol) {
                                case "serial":
                                    return qsTr("Device");
                                case "docker":
                                    return qsTr("Container");
                                case "kube":
                                    return qsTr("Pod");
                                case "s3":
                                    return qsTr("Endpoint");
                                default:
                                    return qsTr("Address");
                                }
                            }
                            helpText: dialog.protocol === "s3" ? qsTr("The server only (empty for AWS): buckets are chosen in the files view.") : ""
                            placeholder: {
                                switch (dialog.protocol) {
                                case "serial":
                                    return qsTr("/dev/ttyUSB0 or COM3");
                                case "docker":
                                    return qsTr("container name or ID");
                                case "kube":
                                    return qsTr("pod name");
                                case "s3":
                                    return qsTr("https://s3.example.com or http://nas:9000");
                                default:
                                    return qsTr("host name or IP address");
                                }
                            }
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "s3.region"
                            inheritKey: ""
                            visible: dialog.protocol === "s3"
                            label: qsTr("Region")
                            placeholder: "us-east-1"
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "s3.path_style"
                            inherit: false
                            visible: dialog.protocol === "s3"
                            label: qsTr("Path-style addresses")
                            options: [{ text: qsTr("Default (on)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Buckets in the path (server/bucket) rather than in the host name (bucket.server): what MinIO, RustFS and most other servers want.")
                        }

                        // Remote desktops.
                        EditorTextRow {
                            editor: dialog
                            path: "rdp.domain"
                            inheritKey: ""
                            visible: dialog.protocol === "rdp"
                            label: qsTr("Domain")
                            placeholder: qsTr("none, or the one in the user name")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "rdp.scaling"
                            inherit: false
                            visible: dialog.protocol === "rdp"
                            label: qsTr("Scaling")
                            options: [{ text: qsTr("Default (follow the pane's size)"), value: undefined },
                                { text: qsTr("Fit in the pane"), value: "fit" }, { text: qsTr("Actual size"), value: "actual" }]
                            helpText: qsTr("Following the pane, the desktop takes the pane's size whenever it changes; otherwise it keeps its resolution and is scaled to fit, or shown pixel for pixel.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "rdp.resolution"
                            inheritKey: ""
                            visible: dialog.protocol === "rdp" && (dialog.draft.rdp ?? {}).scaling !== undefined
                            label: qsTr("Resolution")
                            placeholder: qsTr("the pane's size")
                            helpText: qsTr("Width and height, as in 1920x1080.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "rdp.clipboard"
                            inherit: false
                            visible: dialog.protocol === "rdp"
                            label: qsTr("Share the clipboard")
                            options: [{ text: qsTr("Default (on)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Text copied here can be pasted on the remote desktop, and the other way round.")
                        }

                        // VNC.
                        EditorChoiceRow {
                            editor: dialog
                            path: "vnc.scaling"
                            inherit: false
                            visible: dialog.protocol === "vnc"
                            label: qsTr("Scaling")
                            options: [{ text: qsTr("Default (fit in the pane)"), value: undefined },
                                { text: qsTr("Actual size"), value: "actual" },
                                { text: qsTr("Follow the pane's size"), value: "dynamic" }]
                            helpText: qsTr("Following the pane's size works with servers that resize their desktop, such as TigerVNC; the others keep theirs.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "vnc.quality"
                            inherit: false
                            visible: dialog.protocol === "vnc"
                            label: qsTr("Picture quality")
                            options: [{ text: qsTr("Default (high)"), value: undefined }, { text: qsTr("Lossless"), value: "lossless" },
                                { text: qsTr("High"), value: "high" }, { text: qsTr("Medium"), value: "medium" },
                                { text: qsTr("Low (least bandwidth)"), value: "low" }]
                            helpText: qsTr("Lower qualities send pictures as JPEG, for slow connections; lossless keeps every pixel exact.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "vnc.read_only"
                            inherit: false
                            visible: dialog.protocol === "vnc"
                            label: qsTr("View only")
                            options: [{ text: qsTr("Default (off)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Watch the desktop without sending keys, the mouse or the clipboard.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "vnc.clipboard"
                            inherit: false
                            visible: dialog.protocol === "vnc"
                            label: qsTr("Share the clipboard")
                            options: [{ text: qsTr("Default (on)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Text copied here can be pasted on the remote desktop, and the other way round.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "vnc.shared"
                            inherit: false
                            visible: dialog.protocol === "vnc"
                            label: qsTr("Let other viewers stay")
                            options: [{ text: qsTr("Default (on)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Off asks the server to disconnect the other viewers of this desktop.")
                        }

                        // Containers and pods: what runs them, and the running ones to pick.
                        EditorChoiceRow {
                            editor: dialog
                            path: "container.engine"
                            inherit: false
                            visible: dialog.protocol === "docker"
                            label: qsTr("Engine")
                            options: [{ text: qsTr("Default (Docker)"), value: undefined }, { text: qsTr("Docker"), value: "docker" },
                                { text: qsTr("Podman"), value: "podman" }]
                        }

                        EditorTextRow {
                            id: namespaceRow

                            editor: dialog
                            path: "container.namespace"
                            inheritKey: ""
                            visible: dialog.protocol === "kube"
                            label: qsTr("Namespace")
                            placeholder: qsTr("the context's")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "container.pod_container"
                            inheritKey: ""
                            visible: dialog.protocol === "kube"
                            label: qsTr("Container")
                            placeholder: qsTr("the pod's default")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "container.context"
                            inheritKey: ""
                            visible: dialog.protocol === "kube"
                            label: qsTr("Context")
                            placeholder: qsTr("the current one")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "container.shell"
                            inheritKey: ""
                            visible: dialog.protocol === "docker" || dialog.protocol === "kube"
                            label: qsTr("Shell")
                            placeholder: qsTr("bash where there is one, else sh")
                        }

                        OsFormRow {
                            id: runningRow

                            readonly property bool shown: dialog.visible && (dialog.protocol === "docker" || dialog.protocol === "kube")
                            readonly property string source: dialog.protocol === "kube" ? "kube"
                                                                                        : dialog.revision >= 0 && dialog.value("container.engine") === "podman" ? "podman" : "docker"
                            readonly property string context: dialog.revision >= 0 && source === "kube" ? (dialog.value("container.context") ?? "") : ""
                            readonly property var listing: {
                                const listed = JSON.parse(Platform.containers || "{}");
                                return listed.source === source && (listed.context ?? "") === context ? listed : { items: [], error: "" };
                            }
                            readonly property var items: listing.items ?? []

                            // Every namespace of the context: picking a pod sets its namespace.
                            function refresh() {
                                Platform.refreshContainers(source, context, "");
                            }

                            width: parent.width
                            visible: shown
                            label: dialog.protocol === "kube" ? qsTr("Running pods") : qsTr("Running containers")
                            helpText: (listing.error ?? "").length > 0 ? listing.error
                                                                       : items.length === 0 ? qsTr("None found yet: Refresh lists them again.")
                                                                                            : qsTr("Choose one to use it.")

                            Row {
                                width: parent.width
                                spacing: Theme.spacingSm

                                OsComboBox {
                                    width: parent.width - refreshButton.width - parent.spacing
                                    enabled: !dialog.readOnly && runningRow.items.length > 0
                                    model: runningRow.items.map(item => ({
                                        text: item.detail.length > 0 ? qsTr("%1 (%2)").arg(item.name).arg(item.detail) : item.name
                                    }))
                                    textRole: "text"
                                    currentIndex: dialog.revision >= 0 ? runningRow.items.findIndex(item => item.name === dialog.value("address")
                                                                                                    && (!item.namespace || item.namespace === dialog.value("container.namespace"))) : -1
                                    displayText: currentIndex < 0 ? qsTr("Choose…") : currentText
                                    Accessible.name: runningRow.label

                                    onActivated: index => {
                                        const item = runningRow.items[index];
                                        dialog.setValue("address", item.name);
                                        addressRow.show();
                                        if (item.namespace) {
                                            dialog.setValue("container.namespace", item.namespace);
                                            namespaceRow.show();
                                        }
                                    }
                                }

                                OsButton {
                                    id: refreshButton

                                    text: qsTr("Refresh")
                                    iconName: "refresh-cw"
                                    onClicked: runningRow.refresh()
                                }
                            }

                            onShownChanged: {
                                if (shown)
                                    refresh();
                            }
                            onSourceChanged: {
                                if (shown)
                                    refresh();
                            }
                        }

                        OsFormRow {
                            id: portRow

                            readonly property var ports: JSON.parse(Platform.serialPorts || "[]")

                            width: parent.width
                            visible: dialog.protocol === "serial"
                            label: qsTr("Detected ports")
                            helpText: ports.length === 0 ? qsTr("None found. Plug the device in: the list refreshes by itself.")
                                                         : qsTr("Choose one to use it as the device.")

                            OsComboBox {
                                width: parent.width
                                enabled: !dialog.readOnly && portRow.ports.length > 0
                                model: portRow.ports.map(port => ({
                                    text: port.description.length > 0 ? qsTr("%1 (%2)").arg(port.name).arg(port.description) : port.name
                                }))
                                textRole: "text"
                                currentIndex: dialog.revision >= 0 ? portRow.ports.findIndex(port => port.name === dialog.value("address")) : -1
                                displayText: currentIndex < 0 ? qsTr("Choose a port") : currentText
                                Accessible.name: portRow.label

                                onActivated: index => {
                                    dialog.setValue("address", portRow.ports[index].name);
                                    addressRow.show();
                                }
                            }
                        }

                        // The detected ports, read again every 2 s while a serial host is edited.
                        Timer {
                            interval: 2000
                            repeat: true
                            triggeredOnStart: true
                            running: dialog.visible && dialog.protocol === "serial"
                            onTriggered: Platform.refreshSerialPorts()
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "port"
                            type: "int"
                            visible: ["ssh", "sftp", "telnet", "mosh", "rdp", "vnc"].indexOf(dialog.protocol) >= 0
                            label: qsTr("Port")
                        }

                        OsText {
                            width: parent.width
                            visible: dialog.protocol === "telnet"
                            text: qsTr("Telnet sends everything in clear, passwords too: anyone on the network path can read it. Use SSH where the device has it.")
                            color: Theme.warning
                            wrapMode: Text.Wrap
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

                        EditorChoiceRow {
                            editor: dialog
                            path: "identity"
                            label: qsTr("Identity")
                            options: [{ text: qsTr("None"), value: "none" }].concat(dialog.identityList.map(identity => ({
                                text: identity.user.length > 0 ? qsTr("%1 (%2)").arg(identity.name).arg(identity.user) : identity.name,
                                value: identity.id
                            })))
                            helpText: dialog.protocol === "s3" ? qsTr("For S3, the identity's user name is the access key and its password the secret key.")
                                                               : qsTr("A user name with a password and/or a key from the keychain. The OpenSSH client only uses its user name.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "user"
                            label: dialog.protocol === "s3" ? qsTr("Access key") : qsTr("User")
                            placeholder: dialog.protocol === "s3" ? qsTr("the identity's user name") : qsTr("the identity's, else the local user name")
                        }

                        OsFormRow {
                            width: parent.width
                            visible: dialog.protocol === "s3"
                            label: qsTr("Secret key")
                            helpText: qsTr("Saved encrypted in the vault, as the identity's password (a new identity named after the host when it has none). Left empty, the saved one stays, or it is asked for when connecting.")

                            OsTextField {
                                id: secretField

                                width: parent.width
                                readOnly: dialog.readOnly
                                echoMode: TextInput.Password
                                placeholderText: {
                                    const current = dialog.identityList.find(identity => identity.id === dialog.draft.identity);
                                    return current && current.hasPassword ? qsTr("saved in the vault") : qsTr("not saved");
                                }
                                Accessible.name: qsTr("Secret key")
                            }
                        }

                        EditorTextRow {
                            id: identityRow

                            visible: ["s3", "rdp", "vnc"].indexOf(dialog.protocol) < 0
                            editor: dialog
                            path: "identity_file"
                            label: qsTr("Private key file")
                            placeholder: qsTr("~/.ssh/id_ed25519, id_ecdsa or id_rsa, after the agent's keys")
                            helpText: qsTr("A certificate next to it (key-cert.pub) is used too.")
                        }

                        OsFormRow {
                            width: parent.width
                            visible: identityRow.visible

                            OsButton {
                                text: qsTr("Choose a key file…")
                                iconName: "folder-open"
                                enabled: !dialog.readOnly
                                onClicked: keyDialog.open()
                            }
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.auth_order"
                            type: "list"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "mosh"
                            label: qsTr("Authentication order")
                            helpText: qsTr("The methods to try, in order: publickey (the identity's key, the key file, the agent), keyboard-interactive (one-time codes) and password.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.agent_socket"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "mosh"
                            label: qsTr("Agent")
                            placeholder: qsTr("SSH_AUTH_SOCK, else the system's agent")
                            helpText: qsTr("A socket path or a Windows pipe name (\\\\.\\pipe\\...), for an agent other than the usual one.")
                        }

                        Note {
                            text: qsTr("Passwords and keys stay in the encrypted vault; hosts.toml only names the identity. The vault is opened only when a connection needs them.")
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
                            visible: ["ssh", "sftp", "mosh", "rdp", "vnc"].indexOf(dialog.protocol) >= 0
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
                            helpText: qsTr("The system's OpenSSH client is there for what the built-in one doesn't do (Kerberos, smart cards, Match exec). It uses its own settings: most options here don't apply to it.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.legacy_algorithms"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "mosh"
                            label: qsTr("Legacy algorithms")
                            options: dialog.onOff
                            helpText: qsTr("For old servers: SHA-1 key exchange and signatures, CBC ciphers and hmac-sha1. They are weaker; turn them on only for a server that needs them.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.proxy"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "mosh"
                            label: qsTr("Proxy")
                            placeholder: qsTr("socks5://host:1080 or http://host:8080")
                            helpText: qsTr("For the first hop (a jump host, or this host).")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.proxy_command"
                            visible: dialog.protocol === "ssh" || dialog.protocol === "mosh"
                            label: qsTr("Proxy command")
                            placeholder: qsTr("nc -X connect -x proxy:3128 %h %p")
                            helpText: qsTr("A program whose input and output carry the connection, instead of the proxy (%h host, %p port, %r user).")
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
                            id: x11Row

                            editor: dialog
                            path: "ssh.x11"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("X11 forwarding")
                            options: [{ text: qsTr("Off"), value: "off" }, { text: qsTr("Untrusted"), value: "untrusted" },
                                { text: qsTr("Trusted"), value: "trusted" }]
                            helpText: dialog.revision >= 0 && dialog.value("ssh.x11") === "trusted"
                                      ? qsTr("Trusted forwarding gives remote programs full access to your display: they can read what you type in other windows. Use it only for programs that refuse the untrusted kind.")
                                      : qsTr("Remote X programs show on this computer's display (DISPLAY; on Windows an X server such as VcXsrv). Untrusted keeps them from watching your other windows.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.waypipe"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Waypipe")
                            options: [{ text: qsTr("Off"), value: false }, { text: qsTr("On"), value: true }]
                            helpText: qsTr("Remote Wayland programs show on this computer's Wayland desktop. Needs waypipe installed here and on the server (Linux).")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.env"
                            type: "map"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Environment")
                            placeholder: qsTr("NAME=value; OTHER=value")
                            helpText: qsTr("Sent to the server, which only sets what its AcceptEnv allows.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.send_locale"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Send the language settings")
                            options: dialog.onOff
                            helpText: qsTr("LANG and LC_* from this computer, as OpenSSH sends them.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.command"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Remote command")
                            placeholder: qsTr("the login shell")
                            helpText: qsTr("Runs instead of the shell, like ssh host command.")
                        }

                        EditorTextRow {
                            editor: dialog
                            path: "ssh.startup_snippet"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Startup snippet")
                            helpText: qsTr("Typed once the shell is ready, on every connection.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.auto_reconnect"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Reconnect by itself")
                            options: dialog.onOff
                            helpText: qsTr("After the connection drops, tries again after 1, 2, 4, 8 and 16 seconds. Enter reconnects at any time.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.log"
                            visible: ["ssh", "telnet", "serial"].indexOf(dialog.protocol) >= 0
                            label: qsTr("Session log")
                            options: [{ text: qsTr("Off"), value: "off" }, { text: qsTr("Text"), value: "text" },
                                { text: qsTr("Raw (with escape codes)"), value: "raw" }]
                            helpText: qsTr("Saved in the logs/sessions folder of OpenSesh's data folder, one file per connection.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.detect_os"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Detect the OS")
                            options: dialog.onOff
                            helpText: qsTr("Reads /etc/os-release once connected, for the automatic icon.")
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "ssh.monitor"
                            visible: dialog.protocol === "ssh"
                            label: qsTr("Remote monitor")
                            options: dialog.onOff
                            helpText: qsTr("CPU, memory, network and disks in the status bar, read every few seconds over a separate channel. Nothing is installed on the server.")
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

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.newline"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Enter sends")
                            options: [{ text: qsTr("Default (CR)"), value: undefined }, { text: qsTr("CR"), value: "cr" },
                                { text: qsTr("LF"), value: "lf" }, { text: qsTr("CR LF"), value: "crlf" }]
                        }

                        EditorChoiceRow {
                            editor: dialog
                            path: "serial.local_echo"
                            inherit: false
                            visible: dialog.protocol === "serial"
                            label: qsTr("Local echo")
                            options: [{ text: qsTr("Default (off)"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Shows what you type, for devices that don't send it back.")
                        }

                        OsText {
                            width: parent.width
                            visible: ["ssh", "telnet", "serial", "mosh"].indexOf(dialog.protocol) < 0
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

                        EditorChoiceRow {
                            editor: dialog
                            path: "terminal.paste_protection"
                            inherit: false
                            label: qsTr("Check pastes")
                            options: [{ text: qsTr("From the profile"), value: undefined }].concat(dialog.onOff)
                            helpText: qsTr("Show what looks risky in a paste before sending it (Settings > Terminal).")
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
                                    // A template TextArea keeps a one-line implicit height: it grows with its text.
                                    implicitHeight: contentHeight + topPadding + bottomPadding
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
