pragma ComponentBehavior: Bound

// A sync conflict in the settings folder (Sprint 16): two versions of a file, left by Syncthing
// (a `.sync-conflict-` copy) or by Git (conflict markers). Lists what differs by record (hosts,
// groups, snippets...) and lets you keep either side of each; records on one side only are
// kept by default, so nothing is lost. A file that can't be compared by record (known_hosts)
// keeps one whole side. `SettingsSync` writes the result and removes Syncthing's copy.
// Functions: show(index) (in SettingsSync.conflicts).
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

OsDialog {
    id: dialog

    property int index: -1
    property var details: ({ differences: [], whole: false })
    // "<section>\t<id>" -> "here" | "there"
    property var choices: ({})
    property bool working: false

    function show(conflictIndex) {
        index = conflictIndex;
        details = JSON.parse(SettingsSync.conflictDetails(conflictIndex) || "{}");
        const initial = {};
        for (const difference of details.differences ?? [])
            initial[key(difference)] = difference.change === "only-there" ? "there" : "here";
        if (details.whole)
            initial["*"] = "here";
        choices = initial;
        working = false;
        open();
    }

    function key(difference) {
        return difference.section + "\t" + difference.id;
    }

    function choose(name, side) {
        const next = Object.assign({}, choices);
        next[name] = side;
        choices = next;
    }

    function sectionLabel(section) {
        switch (section) {
        case "host":
            return qsTr("Host");
        case "group":
            return qsTr("Group");
        case "snippet":
            return qsTr("Snippet");
        case "tunnel":
            return qsTr("Tunnel");
        case "source":
            return qsTr("Linked file");
        default:
            return qsTr("Setting %1").arg(section);
        }
    }

    function changeLabel(change) {
        switch (change) {
        case "only-here":
            return qsTr("only on this computer");
        case "only-there":
            return qsTr("only in the other copy");
        default:
            return qsTr("changed on both");
        }
    }

    // What each choice means for a difference.
    function sideLabels(change) {
        switch (change) {
        case "only-here":
            return [qsTr("Keep it"), qsTr("Remove it")];
        case "only-there":
            return [qsTr("Leave it out"), qsTr("Add it")];
        default:
            return [qsTr("Keep this computer's"), qsTr("Take the other")];
        }
    }

    title: qsTr("Resolve a sync conflict")
    acceptText: working ? qsTr("Saving…") : qsTr("Resolve")
    acceptEnabled: !working && !details.error
    closeOnAccept: false
    onAcceptClicked: {
        working = true;
        SettingsSync.resolveConflict(index, JSON.stringify(choices));
    }

    Connections {
        target: SettingsSync

        function onFinished(action, code, detail) {
            if (action !== "resolve" || !dialog.opened)
                return;
            dialog.working = false;
            if (code.length === 0) {
                Toasts.show(qsTr("%1 is resolved.").arg(dialog.details.name), "success");
                dialog.accept();
            } else if (code !== "test-run") {
                Toasts.show(qsTr("The conflict could not be resolved."), "danger", "", "", detail);
            } else {
                dialog.accept();
            }
        }
    }

    Column {
        width: Math.min(Theme.spacingXxl * 17, dialog.maxWidth - dialog.leftPadding - dialog.rightPadding)
        spacing: Theme.spacingMd

        OsText {
            width: parent.width
            wrapMode: Text.Wrap
            text: dialog.details.source === "git" ? qsTr("Git left two versions of %1 after a pull.").arg(dialog.details.name)
                                                   : qsTr("Syncthing kept two versions of %1: it changed here and on another computer before they synced.").arg(dialog.details.name)
        }

        OsText {
            width: parent.width
            visible: (dialog.details.error ?? "").length > 0
            text: qsTr("The versions can't be read: %1").arg(dialog.details.error)
            color: Theme.danger
            wrapMode: Text.Wrap
        }

        OsText {
            width: parent.width
            visible: !dialog.details.whole && (dialog.details.differences ?? []).length === 0 && !dialog.details.error
            text: qsTr("Both versions hold the same: resolving removes the extra copy.")
            muted: true
            wrapMode: Text.Wrap
        }

        // A file compared as a whole.
        Row {
            visible: dialog.details.whole === true && !dialog.details.error
            spacing: Theme.spacingSm

            OsButton {
                text: qsTr("Keep this computer's")
                variant: dialog.choices["*"] === "here" ? "primary" : "secondary"
                onClicked: dialog.choose("*", "here")
            }

            OsButton {
                text: qsTr("Take the other")
                variant: dialog.choices["*"] === "there" ? "primary" : "secondary"
                onClicked: dialog.choose("*", "there")
            }
        }

        ListView {
            width: parent.width
            height: Math.min(contentHeight, Theme.rowHeight * 8)
            visible: count > 0
            clip: true
            spacing: Theme.spacingXs
            model: dialog.details.differences ?? []
            boundsBehavior: Flickable.StopAtBounds
            Accessible.role: Accessible.List
            Accessible.name: qsTr("What differs")

            delegate: OsFormRow {
                id: differenceRow

                required property var modelData
                readonly property string name: dialog.key(modelData)
                readonly property var labels: dialog.sideLabels(modelData.change)

                width: ListView.view.width
                label: modelData.name.length > 0 ? modelData.name : dialog.sectionLabel(modelData.section)
                helpText: qsTr("%1, %2").arg(dialog.sectionLabel(modelData.section)).arg(dialog.changeLabel(modelData.change))

                OsComboBox {
                    width: parent.width
                    model: differenceRow.labels
                    currentIndex: dialog.choices[differenceRow.name] === "there" ? 1 : 0
                    Accessible.name: qsTr("Choice for %1").arg(differenceRow.label)
                    onActivated: index => dialog.choose(differenceRow.name, index === 1 ? "there" : "here")
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }
    }
}
