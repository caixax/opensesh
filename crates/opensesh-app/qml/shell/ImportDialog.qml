pragma ComponentBehavior: Bound

// Import hosts from another program or from an OpenSesh bundle (Sprint 16): MobaXterm, PuTTY,
// Remmina, a CSV file (its columns mapped to host fields, guessed from the headers) or a
// bundle (its snippets, profiles, themes and, with its export password, its keychain). Shows
// what would come and what is left out, then adds the hosts to a new group, their folders as
// groups inside it. `~/.ssh/config` has its own dialog (it can be linked), opened from here.
// Functions: show(source) ("" for MobaXterm), load(source, path) (opens and reads `path`).
import QtQuick
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    required property Item shell
    property string source: "mobaxterm"
    property var preview: ({ hosts: [], warnings: [], groups: 0, error: "" })
    property var csv: ({ rows: [], fields: [], count: 0, error: "" })
    // The field code of each CSV column.
    property var columns: []
    property bool csvHeader: true
    property bool withSnippets: true
    property bool withProfiles: true
    property bool withKeychain: true
    property bool working: false
    property string error: ""

    readonly property var sources: [
        {
            value: "mobaxterm",
            text: qsTr("MobaXterm"),
            hint: qsTr("A .mxtsessions export, a single .moba session or MobaXterm.ini. SSH, SFTP, RDP and VNC sessions are imported.")
        },
        {
            value: "putty",
            text: qsTr("PuTTY"),
            hint: Qt.platform.os === "windows" ? qsTr("Your saved sessions in the registry, or a .reg file exported from another computer. SSH, Telnet and serial sessions are imported.")
                                               : qsTr("The ~/.putty/sessions folder, or a .reg file exported on Windows. SSH, Telnet and serial sessions are imported.")
        },
        {
            value: "remmina",
            text: qsTr("Remmina"),
            hint: qsTr("A folder of .remmina profiles, or one profile. RDP, VNC, SSH and SFTP profiles are imported.")
        },
        {
            value: "csv",
            text: qsTr("CSV file"),
            hint: qsTr("A spreadsheet saved as CSV: map its columns to host fields below.")
        },
        {
            value: "bundle",
            text: qsTr("OpenSesh bundle"),
            hint: qsTr("Hosts, snippets, profiles and themes exported from OpenSesh, and its keychain when it was exported with a password.")
        },
        {
            value: "ssh_config",
            text: qsTr("~/.ssh/config"),
            hint: qsTr("OpenSSH's config file, imported or linked.")
        }
    ]
    readonly property var fieldChoices: [
        { value: "ignore", text: qsTr("Leave out") },
        { value: "name", text: qsTr("Name") },
        { value: "address", text: qsTr("Address") },
        { value: "port", text: qsTr("Port") },
        { value: "user", text: qsTr("User") },
        { value: "protocol", text: qsTr("Protocol") },
        { value: "group", text: qsTr("Group") },
        { value: "tags", text: qsTr("Tags") },
        { value: "notes", text: qsTr("Notes") },
        { value: "identity_file", text: qsTr("Key file") },
        { value: "jump", text: qsTr("Jump hosts") }
    ]
    readonly property var current: sources.find(entry => entry.value === source) ?? sources[0]
    readonly property var hosts: preview.hosts ?? []
    readonly property var bundle: preview.bundle ?? ({})
    readonly property bool hasKeychain: source === "bundle" && bundle.keychain === true
    readonly property bool needsAddress: source === "csv" && columns.indexOf("address") < 0

    function show(which) {
        source = which && which.length > 0 ? which : "mobaxterm";
        error = "";
        working = false;
        passwordField.text = "";
        groupField.text = "";
        columns = [];
        pickSource(source);
        open();
    }

    function load(which, path) {
        if (!opened)
            show(which);
        source = which;
        columns = [];
        pathField.text = path;
        reload();
    }

    function pickSource(value) {
        if (value === "ssh_config") {
            close();
            shell.showSshImport("");
            return;
        }
        source = value;
        columns = [];
        error = "";
        pathField.text = Hosts.importDefaultPath(value);
        reload();
    }

    function reload() {
        error = "";
        const path = pathField.text.trim();
        if (path.length === 0) {
            preview = { hosts: [], warnings: [], groups: 0, error: "" };
            csv = { rows: [], fields: [], count: 0, error: "" };
            return;
        }
        if (source === "csv") {
            csv = JSON.parse(Hosts.csvTable(path) || "{}");
            if (columns.length !== (csv.fields ?? []).length)
                columns = (csv.fields ?? []).slice();
        }
        preview = JSON.parse(Hosts.previewImport(source, path, JSON.stringify(options({}))) || "{}");
    }

    function options(identities) {
        if (source === "csv")
            return { columns: columns, header: csvHeader };
        if (source === "bundle")
            return { identities: identities };
        return {};
    }

    function setColumn(index, value) {
        const next = columns.slice();
        // A field goes to one column only.
        for (let i = 0; i < next.length; ++i) {
            if (i !== index && value !== "ignore" && next[i] === value)
                next[i] = "ignore";
        }
        next[index] = value;
        columns = next;
        reload();
    }

    function submit() {
        if (working || hosts.length === 0)
            return;
        if (hasKeychain && withKeychain) {
            if (passwordField.text.length === 0) {
                error = qsTr("Type the bundle's export password, or leave the keychain out.");
                return;
            }
            if (Keychain.vaultStatus === "locked") {
                shell.unlockVault(() => dialog.submit());
                return;
            }
            working = true;
            KeychainTasks.run(Keychain.importBundle(pathField.text, passwordField.text), (code, detail, value) => {
                dialog.working = false;
                if (code.length > 0) {
                    dialog.error = code === "decrypt" ? qsTr("This password doesn't open the bundle's keychain.")
                                                      : KeychainTasks.message(code, detail);
                    return;
                }
                passwordField.text = "";
                dialog.finish(JSON.parse(value || "{}").identities ?? {});
            });
            return;
        }
        finish({});
    }

    function finish(identities) {
        const path = pathField.text;
        const group = groupField.text.trim().length > 0 ? groupField.text.trim() : groupField.placeholderText;
        const result = JSON.parse(Hosts.importHosts(source, path, JSON.stringify(options(identities)), group) || "{}");
        if (result.error) {
            error = result.error;
            return;
        }
        if (result.added === undefined) {
            Toasts.show(qsTr("The hosts could not be imported."), "danger");
            return;
        }
        const parts = [result.skipped > 0 ? qsTr("%n host(s) imported; %1 left out (saved already, or not valid).", "", result.added).arg(result.skipped)
                                          : qsTr("%n host(s) imported.", "", result.added)];
        if (source === "bundle") {
            if (withSnippets && (bundle.snippets ?? 0) > 0)
                parts.push(qsTr("%n snippet(s) added.", "", Snippets.importBundle(path)));
            if (withProfiles && ((bundle.profiles ?? 0) + (bundle.themes ?? 0)) > 0) {
                const files = JSON.parse(TerminalProfiles.importBundle(path) || "{}");
                parts.push(qsTr("%n profile(s) and %1 theme(s) added.", "", files.profiles ?? 0).arg(files.themes ?? 0));
            }
        }
        Toasts.show(parts.join(" "), "success");
        accept();
    }

    title: qsTr("Import hosts")
    acceptText: working ? qsTr("Importing…") : qsTr("Import")
    acceptEnabled: !working && hosts.length > 0 && !needsAddress
    closeOnAccept: false
    onAcceptClicked: submit()

    // The body scrolls when it is taller than the window allows (a CSV file's many columns).
    Flickable {
        implicitWidth: body.width
        implicitHeight: body.height
        width: body.width
        // The content item, between the header and the footer.
        height: parent ? Math.min(body.height, parent.height) : body.height
        contentWidth: body.width
        contentHeight: body.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        T.ScrollBar.vertical: OsScrollBar {}

        Column {
            id: body

            width: Math.min(Theme.spacingXxl * 18, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
            spacing: Theme.spacingMd

            OsFormRow {
                width: parent.width
                label: qsTr("From")

                OsComboBox {
                    width: parent.width
                    model: dialog.sources
                    textRole: "text"
                    valueRole: "value"
                    currentIndex: dialog.sources.findIndex(entry => entry.value === dialog.source)
                    Accessible.name: qsTr("Import from")
                    onActivated: index => dialog.pickSource(dialog.sources[index].value)
                }
            }

            OsText {
                width: parent.width
                text: dialog.current.hint
                muted: true
                size: "small"
                wrapMode: Text.Wrap
            }

            RowLayout {
                width: parent.width
                spacing: Theme.spacingSm

                OsTextField {
                    id: pathField

                    Layout.fillWidth: true
                    placeholderText: dialog.source === "putty" && Qt.platform.os === "windows" ? qsTr("registry") : qsTr("File or folder")
                    Accessible.name: qsTr("File or folder to import")
                    onEditingFinished: dialog.reload()
                }

                OsButton {
                    text: qsTr("File…")
                    iconName: "file"
                    onClicked: fileDialog.open()
                }

                OsButton {
                    visible: dialog.source === "putty" || dialog.source === "remmina"
                    text: qsTr("Folder…")
                    iconName: "folder-open"
                    onClicked: folderDialog.open()
                }

                OsButton {
                    visible: dialog.source === "putty" && Qt.platform.os === "windows"
                    text: qsTr("Registry")
                    onClicked: {
                        pathField.text = "registry"; // lint-qml: allow (the token Hosts.importHosts reads)
                        dialog.reload();
                    }
                }
            }

            // The CSV columns.
            Column {
                width: parent.width
                visible: dialog.source === "csv" && (dialog.csv.rows ?? []).length > 0
                spacing: Theme.spacingXs

                OsCheckBox {
                    text: qsTr("The first row names the columns")
                    checked: dialog.csvHeader
                    onToggled: {
                        dialog.csvHeader = checked;
                        dialog.reload();
                    }
                }

                ListView {
                    width: parent.width
                    height: Math.min(contentHeight, (Theme.controlHeight + Theme.spacingXl) * 3.5)
                    clip: true
                    spacing: Theme.spacingXs
                    model: dialog.columns.length
                    boundsBehavior: Flickable.StopAtBounds
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("The file's columns")

                    T.ScrollBar.vertical: OsScrollBar {}

                    delegate: OsFormRow {
                        id: columnRow

                        required property int index
                        readonly property var first: (dialog.csv.rows ?? [])[0] ?? []
                        readonly property var second: (dialog.csv.rows ?? [])[1] ?? []

                        width: ListView.view.width - Theme.spacingMd
                        label: dialog.csvHeader ? (first[index] || qsTr("Column %1").arg(index + 1)) : qsTr("Column %1").arg(index + 1)
                        helpText: (dialog.csvHeader ? second[index] : first[index]) ?? ""

                        OsComboBox {
                            width: parent.width
                            model: dialog.fieldChoices
                            textRole: "text"
                            valueRole: "value"
                            currentIndex: dialog.fieldChoices.findIndex(choice => choice.value === dialog.columns[columnRow.index])
                            Accessible.name: qsTr("Field of %1").arg(columnRow.label)
                            onActivated: index => dialog.setColumn(columnRow.index, dialog.fieldChoices[index].value)
                        }
                    }
                }

                OsText {
                    width: parent.width
                    visible: dialog.needsAddress
                    text: qsTr("Choose the column with the addresses.")
                    color: Theme.danger
                    size: "small"
                    wrapMode: Text.Wrap
                }
            }

            OsText {
                width: parent.width
                visible: pathField.text.trim().length > 0
                text: {
                    if (dialog.csv.error || dialog.preview.error)
                        return qsTr("This can't be read: %1").arg(dialog.csv.error || dialog.preview.error);
                    return dialog.hosts.length > 0 ? qsTr("%n host(s) found.", "", dialog.hosts.length) : qsTr("No hosts found here.");
                }
                muted: !(dialog.csv.error || dialog.preview.error)
                color: dialog.csv.error || dialog.preview.error ? Theme.danger : Theme.textMuted
            }

            ListView {
                id: hostList

                width: parent.width
                height: Math.min(contentHeight, Theme.rowHeight * (dialog.source === "csv" ? 3 : 6))
                visible: count > 0
                clip: true
                model: dialog.hosts
                boundsBehavior: Flickable.StopAtBounds
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Hosts to import")

                delegate: OsListRow {
                    id: importRow

                    required property var modelData

                    width: ListView.view.width
                    text: modelData.name
                    subtitle: modelData.folder.length > 0 ? qsTr("%1 in %2").arg(modelData.target).arg(modelData.folder) : modelData.target
                    iconName: {
                        switch (modelData.protocol) {
                        case "rdp":
                        case "vnc":
                            return "monitor";
                        case "serial":
                            return "usb";
                        case "sftp":
                            return "folder-sync";
                        case "telnet":
                            return "network";
                        default:
                            return "server";
                        }
                    }
                    focusPolicy: Qt.NoFocus

                    OsTag {
                        visible: importRow.modelData.saved
                        text: qsTr("Already saved")
                    }
                }

                T.ScrollBar.vertical: OsScrollBar {}
            }

            SettingsNotice {
                width: parent.width
                visible: (dialog.preview.warnings ?? []).length > 0
                kind: "info"
                title: qsTr("Left out")
                lines: (dialog.preview.warnings ?? []).slice(0, 6).concat((dialog.preview.warnings ?? []).length > 6
                                                                          ? [qsTr("…and %n more.", "", dialog.preview.warnings.length - 6)] : [])
            }

            // Programs the import would run on this computer: worth a look before importing a file
        // from someone else.
        SettingsNotice {
            readonly property var runs: (dialog.preview.runs ?? []).concat(dialog.withProfiles ? (dialog.bundle.runs ?? []) : [])

            width: parent.width
            visible: runs.length > 0
            kind: "warning"
            title: qsTr("Runs programs on this computer")
            lines: runs.slice(0, 6).map(run => run.kind === "proxy" ? qsTr("%1 connects by running: %2").arg(run.name).arg(run.command)
                                                                     : qsTr("%1 starts local terminals with: %2").arg(run.name).arg(run.command))
                   .concat(runs.length > 6 ? [qsTr("…and %n more.", "", runs.length - 6)] : [])
        }

        // What else a bundle brings.
            Column {
                width: parent.width
                visible: dialog.source === "bundle" && dialog.hosts.length > 0
                spacing: Theme.spacingXs

                OsCheckBox {
                    visible: (dialog.bundle.snippets ?? 0) > 0
                    text: qsTr("Its %n snippet(s)", "", dialog.bundle.snippets ?? 0)
                    checked: dialog.withSnippets
                    onToggled: dialog.withSnippets = checked
                }

                OsCheckBox {
                    visible: ((dialog.bundle.profiles ?? 0) + (dialog.bundle.themes ?? 0)) > 0
                    text: qsTr("Its %n profile(s) and %1 theme(s) (names you have already are kept as they are)", "", dialog.bundle.profiles ?? 0).arg(dialog.bundle.themes ?? 0)
                    checked: dialog.withProfiles
                    onToggled: dialog.withProfiles = checked
                }

                OsCheckBox {
                    visible: dialog.hasKeychain
                    text: qsTr("Its keychain: identities, keys, passwords")
                    checked: dialog.withKeychain
                    onToggled: dialog.withKeychain = checked
                }

                OsPasswordField {
                    id: passwordField

                    width: parent.width
                    visible: dialog.hasKeychain && dialog.withKeychain
                    placeholderText: qsTr("Export password")
                    Accessible.name: qsTr("The bundle's export password")
                    onAccepted: dialog.submit()
                }
            }

            OsFormRow {
                width: parent.width
                label: qsTr("New group")

                OsTextField {
                    id: groupField

                    width: parent.width
                    placeholderText: dialog.current.text
                    Accessible.name: qsTr("Name of the group for the imported hosts")
                }
            }

            OsText {
                width: parent.width
                visible: dialog.error.length > 0
                text: dialog.error
                color: Theme.danger
                wrapMode: Text.Wrap
            }
        }
    }

    FileDialog {
        id: fileDialog

        title: qsTr("Choose a file to import")
        nameFilters: {
            switch (dialog.source) {
            case "mobaxterm":
                return [qsTr("MobaXterm sessions (*.mxtsessions *.moba *.ini)"), qsTr("All files (*)")];
            case "putty":
                return [qsTr("Registry exports (*.reg)"), qsTr("All files (*)")];
            case "remmina":
                return [qsTr("Remmina profiles (*.remmina)"), qsTr("All files (*)")];
            case "csv":
                return [qsTr("CSV files (*.csv *.txt)"), qsTr("All files (*)")];
            case "bundle":
                return [qsTr("OpenSesh bundles (*.opensesh)"), qsTr("All files (*)")];
            default:
                return [qsTr("All files (*)")];
            }
        }
        onAccepted: {
            pathField.text = Platform.localPath(selectedFile);
            dialog.columns = [];
            dialog.reload();
        }
    }

    FolderDialog {
        id: folderDialog

        title: qsTr("Choose a folder to import")
        onAccepted: {
            pathField.text = Platform.localPath(selectedFolder);
            dialog.reload();
        }
    }
}
