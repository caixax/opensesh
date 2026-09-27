pragma ComponentBehavior: Bound

// The transfer queue (Sprint 8): every job with its progress, speed and time left, and pause,
// resume, cancel and retry; the files being edited, with what happened to their last save; and
// "Clear finished".
//   compact: bool   fewer details (the status bar's popup)
import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import cc.caixa.opensesh

Item {
    id: panel

    property bool compact: false
    readonly property var jobs: JSON.parse(Transfers.jobs || "[]").slice().reverse()
    readonly property var edits: JSON.parse(Transfers.edits || "[]")
    readonly property bool anyFinished: jobs.some(job => job.state === "done" || job.state === "failed" || job.state === "cancelled")

    implicitHeight: content.implicitHeight

    function stateText(job) {
        switch (job.state) {
        case "queued":
            return qsTr("Waiting");
        case "scanning":
            return qsTr("Looking at the folders…");
        case "paused":
            return qsTr("Paused");
        case "asking":
            return qsTr("Waiting for your answer");
        case "done":
            return job.filesSkipped > 0 ? qsTr("Done (%n skipped)", "", job.filesSkipped) : qsTr("Done");
        case "failed":
            return qsTr("Failed: %1").arg(job.error);
        case "cancelled":
            return qsTr("Cancelled");
        default: {
            const parts = [qsTr("%1 of %2").arg(FileFormat.size(job.bytesDone)).arg(FileFormat.size(job.bytesTotal))];
            if (job.speed > 0)
                parts.push(FileFormat.speed(job.speed));
            if (job.eta !== null && job.eta !== undefined)
                parts.push(qsTr("%1 left").arg(FileFormat.duration(job.eta)));
            return parts.join(" · ");
        }
        }
    }

    function editText(edit) {
        switch (edit.state) {
        case "downloading":
            return qsTr("Downloading…");
        case "uploading":
            return qsTr("Saving to the server…");
        case "saved":
            return qsTr("Saved to the server");
        case "conflict":
            return qsTr("Changed on the server since you opened it");
        case "denied":
            return qsTr("The server refused the save");
        case "failed":
            return qsTr("Failed: %1").arg(edit.detail);
        default:
            return qsTr("Saves go to the server");
        }
    }

    ColumnLayout {
        id: content

        anchors.fill: parent
        spacing: Theme.spacingXs

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingSm
            Layout.rightMargin: Theme.spacingXs
            spacing: Theme.spacingSm

            OsText {
                Layout.fillWidth: true
                text: Transfers.active > 0 ? qsTr("Transfers (%1 active)").arg(Transfers.active) : qsTr("Transfers")
                font.weight: Font.DemiBold
            }

            OsButton {
                visible: panel.anyFinished
                variant: "ghost"
                text: qsTr("Clear finished")
                onClicked: Transfers.clearFinished()
            }
        }

        OsText {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacingSm
            visible: panel.jobs.length === 0 && panel.edits.length === 0
            text: qsTr("Nothing is being transferred. Drag files between the panes, or drop them here from your file manager.")
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            muted: true
            size: "small"
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: contentHeight
            clip: true
            spacing: Theme.spacingXs
            boundsBehavior: Flickable.StopAtBounds
            model: panel.edits.concat(panel.jobs)
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Transfers")

            delegate: Rectangle {
                id: item

                required property var modelData
                readonly property bool isEdit: modelData.remote !== undefined
                readonly property bool finished: ["done", "failed", "cancelled"].indexOf(modelData.state) >= 0

                width: ListView.view.width
                height: itemRow.implicitHeight + 2 * Theme.spacingSm
                radius: Theme.radiusControl
                color: Theme.surface2

                RowLayout {
                    id: itemRow

                    anchors.fill: parent
                    anchors.margins: Theme.spacingSm
                    spacing: Theme.spacingSm

                    OsIcon {
                        name: item.isEdit ? "pencil"
                            : item.modelData.fromRemote && !item.modelData.toRemote ? "download"
                            : !item.modelData.fromRemote && item.modelData.toRemote ? "upload" : "arrow-left-right"
                        size: Theme.iconSize
                        color: item.modelData.state === "failed" || item.modelData.state === "conflict" || item.modelData.state === "denied"
                               ? Theme.danger : Theme.textMuted
                    }

                    Column {
                        Layout.fillWidth: true
                        spacing: Theme.spacingXs

                        OsText {
                            width: parent.width
                            text: item.isEdit ? item.modelData.name
                                              : item.modelData.destination.length > 0 && !panel.compact
                                                ? qsTr("%1 → %2").arg(item.modelData.label).arg(item.modelData.destination)
                                                : item.modelData.label
                            elide: Text.ElideMiddle
                        }

                        OsProgress {
                            width: parent.width
                            visible: !item.isEdit && !item.finished
                            indeterminate: item.modelData.state === "scanning" || item.modelData.bytesTotal === 0
                            from: 0
                            to: Math.max(1, item.modelData.bytesTotal ?? 1)
                            value: item.modelData.bytesDone ?? 0
                            Accessible.name: qsTr("Progress of %1").arg(item.modelData.label ?? "")
                        }

                        OsText {
                            width: parent.width
                            text: item.isEdit ? panel.editText(item.modelData) : panel.stateText(item.modelData)
                            size: "small"
                            muted: true
                            elide: Text.ElideRight
                        }

                        OsText {
                            width: parent.width
                            visible: !panel.compact && !item.isEdit && item.modelData.state === "running" && item.modelData.current.length > 0
                            text: qsTr("%1 (%2 of %3 files)").arg(item.modelData.current ?? "").arg(item.modelData.filesDone ?? 0).arg(item.modelData.filesTotal ?? 0)
                            size: "small"
                            muted: true
                            elide: Text.ElideMiddle
                        }
                    }

                    OsIconButton {
                        visible: !item.isEdit && (item.modelData.state === "running" || item.modelData.state === "queued")
                        iconName: "pause"
                        toolTip: qsTr("Pause")
                        onClicked: Transfers.pause(item.modelData.id)
                    }

                    OsIconButton {
                        visible: !item.isEdit && item.modelData.state === "paused"
                        iconName: "play"
                        toolTip: qsTr("Resume")
                        onClicked: Transfers.resume(item.modelData.id)
                    }

                    OsIconButton {
                        visible: !item.isEdit && (item.modelData.state === "failed" || item.modelData.state === "cancelled")
                        iconName: "rotate-ccw"
                        toolTip: qsTr("Try again (copied files are skipped, partial ones continued)")
                        onClicked: Transfers.retry(item.modelData.id)
                    }

                    OsIconButton {
                        visible: item.isEdit || !item.finished
                        iconName: "x"
                        toolTip: item.isEdit ? qsTr("Stop editing (the local copy is deleted)") : qsTr("Cancel")
                        onClicked: {
                            if (item.isEdit)
                                Transfers.stopEdit(item.modelData.id);
                            else
                                Transfers.cancel(item.modelData.id);
                        }
                    }
                }
            }

            T.ScrollBar.vertical: OsScrollBar {}
        }
    }
}
