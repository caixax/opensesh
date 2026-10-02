pragma ComponentBehavior: Bound

// SFTP view (PLAN §5.4, Sprint 8): two file panes side by side, each showing this computer or a
// server (a saved host, or quick-connect text, on a connection of its own), and the transfer
// queue below. F5 and F6 copy or move the selection to the other pane; files drag between the
// panes and in from the file manager. The sides can swap (a server's side connects again).
// Functions: setSource(side, source) (`source`: {mode, hostId, target, title, startPath}),
// pane(side), swapSides(), smokeSteps(smoke).
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

    // Each side's files go to the other side, in the folder they were showing.
    function swapSides() {
        const where = side => Object.assign({}, side.source, {
            startPath: side.pane && side.pane.ready ? side.pane.browser.path : (side.source.startPath ?? "")
        });
        const left = where(leftSide);
        leftSide.use(where(rightSide));
        rightSide.use(left);
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
        let edit = 0;
        let editLocal = "";
        const editEvents = [];
        const onEditReady = (id, local, opened) => {
            if (id === edit)
                editLocal = opened ? local : "";
        };
        const onEditEvent = (id, what) => {
            if (id === edit)
                editEvents.push(what);
        };
        const saves = () => editEvents.filter(what => what === "saved").length;
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
            // 10,000 files: how long they take to list and to sort.
            () => {
                const started = Date.now();
                right.navigate("/many");
                return wait("10,000 files", () => right.browser.count === 10000, () => {
                    const listed = Date.now() - started;
                    const sorting = Date.now();
                    right.browser.sortKey = "modified";
                    right.browser.sortAscending = false;
                    const sorted = Date.now() - sorting;
                    right.browser.sortKey = "name";
                    right.browser.sortAscending = true;
                    console.info("smoke test: 10,000 files listed in", listed, "ms and sorted in", sorted, "ms");
                    return [];
                });
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
                // The same file again: a question, answered "keep both" (a test run never writes
                // the settings).
                AppSettings.sftpPolicy = "ask";
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
            // Editing a server's file: a private copy (in the test folder; a test run opens no
            // editor), each save uploaded, a server copy changed meanwhile is a conflict, and
            // "replace it with mine" uploads anyway.
            () => {
                Transfers.editReady.connect(onEditReady);
                Transfers.editEvent.connect(onEditEvent);
                edit = Transfers.edit(right.browser.paneId, "/docs/readme.txt");
                if (edit <= 0)
                    smoke.fail("editing a server's file didn't start");
                return wait("the private copy", () => editLocal.length > 0);
            },
            () => {
                if (editLocal.indexOf(AppInfo.testFolder()) !== 0)
                    smoke.fail("the private copy is outside the test folder: " + editLocal);
                if (!AppInfo.writeTestFile(editLocal, "edited in the smoke test\n"))
                    smoke.fail("the private copy couldn't be written");
                return wait("the save to reach the server", () => saves() === 1);
            },
            () => {
                right.navigate("/docs");
                return wait("the saved file", () => right.browser.path === "/docs" && entry(right, "readme.txt").size === 25);
            },
            () => {
                // Someone else changes the server's copy.
                right.browser.remove(right.browser.pathsOf(["readme.txt"]));
                return wait("the server's copy to go", () => right.browser.rowOf("readme.txt") < 0, () => {
                    right.browser.createFile("readme.txt");
                    return wait("another server copy", () => right.browser.rowOf("readme.txt") >= 0);
                });
            },
            () => {
                AppInfo.writeTestFile(editLocal, "mine\n");
                return wait("the conflict", () => editEvents.indexOf("conflict") >= 0);
            },
            () => {
                Transfers.resolveEdit(edit, "overwrite");
                return wait("the save after the conflict", () => saves() === 2, () => {
                    right.browser.refresh();
                    return wait("the server's copy replaced", () => entry(right, "readme.txt").size === 5);
                });
            },
            () => {
                Transfers.stopEdit(edit);
                Transfers.editReady.disconnect(onEditReady);
                Transfers.editEvent.disconnect(onEditEvent);
                console.info("smoke test: a server's file was edited: saved, changed on the server meanwhile, replaced");
                right.navigate("/incoming");
                return wait("the upload folder again", () => right.browser.path === "/incoming");
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
                // Swapped: the server on the left (connecting again), this computer on the right,
                // each in the folder it showed.
                const folder = left.browser.path;
                view.swapSides();
                right = view.pane(0);
                left = view.pane(1);
                deadline = Date.now() + timeout;
                return [connectRight, () => wait("the swapped sides", () => left.ready && left.browser.path === folder && right.browser.path === "/",
                                                 () => console.info("smoke test: the SFTP view swapped its sides"))];
            },
            // S3 storage (its in-process test server): the buckets, a bucket's folders, an
            // upload, a temporary link, a folder downloaded, a folder made, renamed and deleted.
            () => {
                if (AppInfo.startS3TestServer() <= 0)
                    smoke.fail("the S3 test server didn't start");
                view.setSource(0, leftSide.initial);
                view.setSource(1, { mode: "remote", hostId: "", target: "s3://smoke@storage.example/media", title: "S3" }); // lint-qml: allow (a quick-connect URL)
                left = view.pane(0);
                right = view.pane(1);
                return wait("the S3 pane", () => left !== null && left.ready && right !== null
                            && (right.ready || right.browser.status === "error"));
            },
            () => {
                if (right.browser.status === "error")
                    smoke.fail("the S3 pane couldn't open: " + right.browser.error + " " + right.browser.errorDetail);
                if (right.browser.storage !== "s3" || !right.s3)
                    smoke.fail("the right pane isn't S3 storage: " + right.browser.storage);
                if (right.browser.path !== "/media" || right.browser.rowOf("photos") < 0 || right.browser.rowOf("notes.txt") < 0)
                    smoke.fail("the S3 pane started at " + right.browser.path + " with " + right.browser.allNames().join(", "));
                right.browser.done.connect(failOnError);
                left.navigate(AppInfo.testFolder() + left.browser.separator + "local");
                right.navigate("/");
                return wait("the buckets and the local folder", () => right.browser.path === "/" && right.browser.rowOf("backups") >= 0
                            && right.browser.rowOf("media") >= 0 && left.browser.rowOf("notes.txt") >= 0);
            },
            () => {
                right.navigate("/backups");
                return wait("a bucket", () => right.browser.path === "/backups" && right.browser.rowOf("db") >= 0);
            },
            () => {
                job = Transfers.copy(left.browser.paneId, left.browser.pathsOf(["notes.txt"]), right.browser.paneId, "/backups", false);
                if (job <= 0)
                    smoke.fail("an upload to S3 wasn't queued");
                return wait("the upload to S3", () => jobState(job) === "done" && right.browser.rowOf("notes.txt") >= 0);
            },
            () => {
                if (entry(right, "notes.txt").size !== 10000)
                    smoke.fail("the S3 object's size is " + entry(right, "notes.txt").size);
                let link = "";
                const token = right.browser.temporaryLink(right.browser.rowOf("notes.txt"), 3600);
                const onLink = (done, code, detail) => {
                    if (done === token)
                        link = code.length === 0 ? detail : "failed: " + code;
                };
                right.browser.done.connect(onLink);
                return wait("a temporary link", () => link.length > 0, () => {
                    right.browser.done.disconnect(onLink);
                    if (link.indexOf("/backups/notes.txt?") < 0 || link.indexOf("X-Amz-Expires=3600") < 0)
                        smoke.fail("the temporary link is " + link);
                    return [];
                });
            },
            () => {
                job = Transfers.copy(right.browser.paneId, ["/media/photos"], left.browser.paneId, left.browser.path, false);
                return wait("a folder downloaded from S3", () => jobState(job) === "done" && left.browser.rowOf("photos") >= 0);
            },
            () => {
                left.browser.remove(left.browser.pathsOf(["photos"]));
                right.browser.mkdir("albums");
                return wait("the local copy deleted and a new S3 folder", () => left.browser.rowOf("photos") < 0 && right.browser.rowOf("albums") >= 0);
            },
            () => {
                right.browser.rename(right.browser.rowOf("albums"), "albums-2026");
                return wait("the S3 folder renamed", () => right.browser.rowOf("albums-2026") >= 0 && right.browser.rowOf("albums") < 0);
            },
            () => {
                right.browser.remove(right.browser.pathsOf(["albums-2026", "notes.txt"]));
                return wait("the S3 folder and object deleted", () => right.browser.rowOf("albums-2026") < 0 && right.browser.rowOf("notes.txt") < 0);
            },
            () => {
                right.browser.done.disconnect(failOnError);
                Transfers.clearFinished();
                console.info("smoke test: the S3 view listed buckets and folders, uploaded, made a temporary link, downloaded a folder, made, renamed and deleted one");
                view.setSource(0, leftSide.initial);
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
            .filter(host => host.protocol === "ssh" || host.protocol === "sftp" || host.protocol === "s3") : []
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
                name: side.source.mode === "local" ? "hard-drive" : side.pane && side.pane.s3 ? "cloud" : "server"
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
                visible: side.other === rightSide
                iconName: "arrow-left-right"
                toolTip: qsTr("Swap the sides")
                onClicked: view.swapSides()
            }

            OsIconButton {
                iconName: side.other === rightSide ? "arrow-right" : "arrow-left"
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
            placeholderText: qsTr("user@host:port, or s3://access_key@host:port")
            Accessible.name: qsTr("Where to connect")
            onAccepted: {
                if (targetDialog.acceptEnabled)
                    targetDialog.accept();
            }
        }
    }
}
