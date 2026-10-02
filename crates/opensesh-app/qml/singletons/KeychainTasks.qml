pragma Singleton

// Keychain operations run on a worker thread: each Keychain call returns a token and
// Keychain.finished(token, code, detail, value) reports how it went. run(token, done) calls
// done(code, detail, value) for that token; message(code, detail) words an error code.
// Functions: run(token, done), message(code, detail), waitSeconds() (0 when a master password
// may be tried now).
import QtQuick
import cc.caixa.opensesh

QtObject {
    id: tasks

    // token -> callback
    property var pending: ({})
    // Ticks while a wait after wrong passwords runs, so countdowns update.
    property real now: Date.now()

    readonly property Connections finishedConnection: Connections {
        target: Keychain

        function onFinished(token, code, detail, value) {
            const done = tasks.pending[token];
            if (done) {
                delete tasks.pending[token];
                done(code, detail, value);
            }
        }
    }

    readonly property Timer clock: Timer {
        interval: 500
        repeat: true
        triggeredOnStart: true
        running: Keychain.waitUntil > tasks.now
        onTriggered: tasks.now = Date.now()
    }

    function run(token, done) {
        if (token > 0 && done)
            pending[token] = done;
        return token;
    }

    function waitSeconds() {
        return Math.max(0, Math.ceil((Keychain.waitUntil - now) / 1000));
    }

    function message(code, detail) {
        const seconds = parseInt(detail) || 0;
        switch (code) {
        case "":
            return "";
        case "wrong-password":
            return seconds > 0 ? qsTr("Wrong master password. Wait %n second(s) before the next try.", "", seconds)
                               : qsTr("Wrong master password.");
        case "wait":
            return qsTr("Too many wrong passwords. Try again in %n second(s).", "", seconds);
        case "locked":
            return qsTr("The vault is locked.");
        case "no-vault":
            return qsTr("There is no vault yet.");
        case "needs-vault":
            return qsTr("This system has no keyring to keep secrets in: set a master password first.");
        case "keyring":
            return qsTr("The system keyring failed: %1").arg(detail);
        case "unreadable":
            return qsTr("The vault can't be read.");
        case "decrypt":
            return qsTr("The password doesn't open it, or the file was changed.");
        case "read":
            return qsTr("The file could not be read: %1").arg(detail);
        case "write":
            return qsTr("Could not save: %1").arg(detail);
        case "read-only":
            return qsTr("keychain.toml is read-only (it could not be read, or a newer OpenSesh wrote it).");
        case "needs-passphrase":
            return qsTr("This key is protected by a passphrase: type it and import again.");
        case "wrong-passphrase":
            return qsTr("Wrong passphrase.");
        case "legacy-pem":
            return qsTr("This key is in the old PEM format. Convert it with “ssh-keygen -p -f <file>” and import it again.");
        case "unsupported":
            return qsTr("This key can't be used: %1").arg(detail);
        case "not-a-key":
            return qsTr("This isn't an OpenSSH or PuTTY private key.");
        case "damaged":
            return qsTr("The key is damaged: %1").arg(detail);
        case "generate":
            return qsTr("The key could not be generated: %1").arg(detail);
        case "duplicate":
            return qsTr("This key is already in the keychain.");
        case "not-found":
            return qsTr("It isn't in the keychain any more.");
        case "test-run":
            return qsTr("Test runs don't write files.");
        case "unavailable":
            return qsTr("The keychain isn't available.");
        default:
            return detail.length > 0 ? detail : qsTr("Something went wrong.");
        }
    }
}
