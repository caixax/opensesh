pragma ComponentBehavior: Bound

// SFTP view (PLAN §5.4, Sprint 8): two file panes side by side, each showing this computer or a
// server (a saved host, or quick-connect text, on a connection of its own), and the transfer
// queue below. F5 and F6 copy or move the selection to the other pane; files drag between the
// panes and in from the file manager.
// Functions: setSource(side, source) (`source`: {mode, hostId, target, title}), pane(side),
// smokeSteps(smoke).
import QtQuick
import QtQuick.Layouts
import cc.caixa.opensesh

Item {
    id: view

    readonly property Item shell: WindowRegistry.mainShell
    property bool showTransfers: true

    function pane(side) {
        return side === 0 ? leftSide.pane : rightSide.pane;
    }

    function setSource(side, source) {
        (side === 0 ? leftSide : rightSide).use(source);
    }

    // Functions for SmokeTest.steps, after the SSH steps started the test server and loaded the
    // hosts fixture: the smoke test's local folder on the left, the saved host H00000 on the right
    // (the test server's folder, on a connection of its own); then a folder made, a file uploaded,
    // uploaded again (a question, "keep both"), renamed, made read-only and writable again,
    // downloaded, and everything deleted. Any error of a pane fails the run.
    function smokeSteps(smoke) {
        const timeout = 15000;
        let deadline = 0;
        let left = null;
        let right = null;
        let answered = -1;
        let job = 0;
        const failOnError = (token, code, detail) => {
            if (code.length > 0)
                smoke.fail("an SFTP pane reported " + code + " " + detail);
        };
        const waitFor = (what, condition, next) => {
            const poll = () => {
                if (condition())
                    return next ? next() : [];
                if (Date.now() > deadline) {
                    smoke.fail("timed out after " + timeout / 1000 + " s waiting for " + what);
                    return [];
                }
                return [poll];
            };
            return poll;
        };
        const wait = (what, condition, next) => {
            deadline = Date.now() + timeout;
            return [waitFor(what, condition, next)];
        };
        const entry = (pane, name) => JSON.parse(pane.browser.entryJson(pane.browser.rowOf(name)) || "{}");
        const jobState = id => {
            const found = JSON.parse(Transfers.jobs || "[]").find(item => item.id === id);
            return found ? found.state : "";
        };
        // Answers the right pane's questions (host key, password) until it lists its files.
        const connectRight = () => {
            const question = right.browser.prompt.length > 0 ? JSON.parse(right.browser.prompt) : {};
            if (question.id !== undefined && question.id !== answered) {
                answered = question.id;
                if (question.kind === "hostKey")
                    right.browser.answerPrompt(question.id, "trust-once", []);
                else if (question.kind === "password")
                    right.browser.answerPrompt(question.id, "submit", ["right password"]); // lint-qml: allow (the test server's password)
                else
                    smoke.fail("the SFTP pane asked something unexpected: " + right.browser.prompt);
            }
            if (right.browser.status === "error")
                smoke.fail("the SFTP pane couldn't connect: " + right.browser.error + " " + right.browser.errorDetail);
            if (right.ready)
                return [];
            if (Date.now() > deadline) {
                smoke.fail("timed out after " + timeout / 1000 + " s waiting for the SFTP pane to connect");
                return [];
            }
            return [connectRight];
        };
        return [
            () => {
                left = view.pane(0);
                return wait("the local pane", () => left !== null && left.ready);
            },
            () => {
                left.browser.done.connect(failOnError);
                left.navigate(AppInfo.testFolder() + left.browser.separator + "local");
                return wait("the smoke test's local folder", () => left.browser.rowOf("notes.txt") >= 0);
            },
            () => {
                view.setSource(1, { mode: "remote", hostId: "H00000", target: "", title: "H00000" }); // lint-qml: allow (a fixture host id)
                right = view.pane(1);
                if (!right || right.browser.remote !== true)
                    smoke.fail("the right pane isn't a server's");
                deadline = Date.now() + timeout;
                return [connectRight];
            },
            () => {
                right.browser.done.connect(failOnError);
                if (right.browser.rowOf("docs") < 0 || right.browser.rowOf("logs") < 0)
                    smoke.fail("the server's folder lists " + right.browser.allNames().join(", "));
                if (right.browser.rowOf(".profile") >= 0 || right.browser.hiddenCount < 1)
                    smoke.fail("a hidden file is listed, or not counted");
                right.browser.mkdir("incoming");
                return wait("the new folder", () => right.browser.rowOf("incoming") >= 0);
            },
            () => {
                right.navigate("/incoming");
                return wait("the new folder to open", () => right.browser.path === "/incoming" && right.browser.count === 0);
            },
            () => {
                job = Transfers.copy(left.browser.paneId, left.browser.pathsOf(["notes.txt"]), right.browser.paneId, "/incoming", false);
                if (job <= 0)
                    smoke.fail("an upload wasn't queued");
                return wait("the upload", () => jobState(job) === "done" && right.browser.rowOf("notes.txt") >= 0);
            },
            () => {
                if (entry(right, "notes.txt").size !== 10000)
                    smoke.fail("the uploaded file's size is " + entry(right, "notes.txt").size);
                if (AppSettings.sftpPolicy !== "ask")
                    return [];
                // The same file again: a question, answered "keep both".
                job = Transfers.copy(left.browser.paneId, left.browser.pathsOf(["notes.txt"]), right.browser.paneId, "/incoming", false);
                return wait("the question about the file already there", () => Transfers.question.indexOf("notes.txt") >= 0, () => {
                    Transfers.answer(job, "rename", false);
                    return wait("the second copy", () => jobState(job) === "done" && right.browser.rowOf("notes (1).txt") >= 0,
                                () => console.info("smoke test: a file already there was asked about and kept both"));
                });
            },
            () => {
                right.browser.rename(right.browser.rowOf("notes.txt"), "renamed.txt");
                return wait("the renamed file", () => right.browser.rowOf("renamed.txt") >= 0);
            },
            () => {
                right.browser.chmod(right.browser.pathsOf(["renamed.txt"]), 0o444);
                return wait("the read-only file", () => (entry(right, "renamed.txt").mode & 0o222) === 0);
            },
            () => {
                right.browser.chmod(right.browser.pathsOf(["renamed.txt"]), 0o644);
                return wait("the writable file", () => (entry(right, "renamed.txt").mode & 0o200) !== 0);
            },
            () => {
                job = Transfers.copy(right.browser.paneId, right.browser.pathsOf(["renamed.txt"]), left.browser.paneId, left.browser.path, false);
                return wait("the download", () => jobState(job) === "done" && left.browser.rowOf("renamed.txt") >= 0);
            },
            () => {
                if (entry(left, "renamed.txt").size !== 10000)
                    smoke.fail("the downloaded file's size is " + entry(left, "renamed.txt").size);
                left.browser.remove(left.browser.pathsOf(["renamed.txt"]));
                right.navigate("/");
                return wait("the local file deleted and the server's home", () => left.browser.rowOf("renamed.txt") < 0 && right.browser.path === "/"
                            && right.browser.rowOf("incoming") >= 0);
            },
            () => {
                right.browser.remove(["/incoming"]);
                return wait("the server's folder deleted", () => right.browser.rowOf("incoming") < 0);
            },
            () => {
                Transfers.clearFinished();
                return wait("finished transfers to leave the queue", () => JSON.parse(Transfers.jobs || "[]").length === 0);
            },
            () => {
                left.browser.done.disconnect(failOnError);
                right.browser.done.disconnect(failOnError);
                console.info("smoke test: the SFTP view listed, made, uploaded, downloaded, renamed, changed and deleted files on the test server");
                view.setSource(1, rightSide.initial);
            }
        ];
    }

    RowLayout {
        id: sides

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: transfersArea.top
        spacing: 0

        Side {
            id: leftSide

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            other: rightSide
            initial: ({ mode: "local", hostId: "", target: "", title: qsTr("This computer") })
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: Theme.borderWidth
            color: Theme.border
        }

        Side {
            id: rightSide

            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            other: leftSide
            initial: ({ mode: "remote", hostId: "", target: "", title: "" })
        }
    }

    // The queue, below a line with its switch.
    Rectangle {
        id: transfersArea

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: view.showTransfers ? Math.min(Theme.spacingXxl * 7, view.height / 3) : transfersToggle.height
        color: Theme.surface

        Rectangle {
            width: parent.width
            height: Theme.borderWidth
            color: Theme.border
        }

        OsIconButton {
            id: transfersToggle

            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Theme.spacingXs
            z: 1
            iconName: view.showTransfers ? "chevron-down" : "chevron-up"
            toolTip: view.showTransfers ? qsTr("Hide the transfers") : qsTr("Show the transfers")
            onClicked: view.showTransfers = !view.showTransfers
        }

        TransfersPanel {
            anchors.fill: parent
            anchors.margins: Theme.spacingXs
            anchors.rightMargin: transfersToggle.width + Theme.spacingSm
            visible: view.showTransfers
        }
    }

    // One side: where its files come from, and the pane.
    component Side: ColumnLayout {
        id: side

        required property Item other
        required property var initial
        property var source: initial
        readonly property Item pane: loader.item
        // Saved hosts with files to browse (the list follows edits of hosts.toml).
        readonly property var hosts: Hosts.revision >= 0 ? JSON.parse(Hosts.search("", "all", "", "", "name") || "[]")
            .filter(host => host.protocol === "ssh" || host.protocol === "sftp") : []
        readonly property var choices: [{ text: qsTr("This computer"), value: "local" }]
            .concat(hosts.map(host => ({ text: host.name, value: "host:" + host.id })))
            .concat([{ text: qsTr("Connect to user@host…"), value: "other" }])

        function use(next) {
            source = next;
            loader.active = false;
            loader.active = true;
        }

        function choose(value) {
            if (value === "local") {
                use({ mode: "local", hostId: "", target: "", title: qsTr("This computer") });
            } else if (value === "other") {
                targetDialog.side = side;
                targetDialog.open();
            } else if (value.startsWith("host:")) {
                const id = value.slice(5);
                const host = side.hosts.find(entry => entry.id === id);
                use({ mode: "remote", hostId: id, target: "", title: host ? host.name : id });
            }
        }

        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.spacingXs
            spacing: Theme.spacingSm

            OsIcon {
                name: side.source.mode === "local" ? "hard-drive" : "server"
                size: Theme.iconSize
                color: Theme.textMuted
            }

            OsComboBox {
                id: picker

                Layout.fillWidth: true
                model: side.choices
                textRole: "text"
                valueRole: "value"
                displayText: side.source.mode === "local" ? qsTr("This computer")
                           : side.source.title.length > 0 ? side.source.title : qsTr("Choose a host")
                Accessible.name: qsTr("Files of")
                onActivated: index => side.choose(side.choices[index].value)
            }

            OsIconButton {
                iconName: "arrow-left-right"
                toolTip: side.other === rightSide ? qsTr("Copy the selection to the right (F5)") : qsTr("Copy the selection to the left (F5)")
                enabled: side.pane !== null && side.pane.ready && side.other.pane !== null && side.other.pane.ready
                onClicked: side.pane.copyToPeer(false)
            }
        }

        Loader {
            id: loader

            Layout.fillWidth: true
            Layout.fillHeight: true

            sourceComponent: FilePane {
                mode: side.source.mode
                hostId: side.source.hostId
                target: side.source.target
                title: side.source.title
                startPath: side.source.startPath ?? ""
                peer: side.other.pane
                onChooseSource: picker.popup.open()
            }
        }
    }

    OsDialog {
        id: targetDialog

        property Item side: null

        title: qsTr("Connect to")
        acceptText: qsTr("Connect")
        acceptEnabled: targetField.text.trim().length > 0

        onOpened: {
            targetField.text = "";
            targetField.forceActiveFocus();
        }
        onAccepted: {
            const text = targetField.text.trim();
            if (side)
                side.use({ mode: "remote", hostId: "", target: text, title: text });
        }

        OsTextField {
            id: targetField

            implicitWidth: Math.min(Theme.spacingXxl * 10, targetDialog.maxWidth - targetDialog.leftPadding - targetDialog.rightPadding)
            placeholderText: qsTr("user@host:port")
            Accessible.name: qsTr("Where to connect")
            onAccepted: {
                if (targetDialog.acceptEnabled)
                    targetDialog.accept();
            }
        }
    }
}
