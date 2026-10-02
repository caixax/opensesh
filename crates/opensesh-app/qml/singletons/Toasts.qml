pragma Singleton

// In-app notifications: transient toasts plus the history shown in the notifications panel.
import QtQuick
import cc.caixa.opensesh

QtObject {
    id: toasts

    // Newest first: [{ id, text, kind, actionText, actionId, details, time }].
    property var history: []
    property int unread: 0

    // Emitted for every new toast; the shell's toast host shows it.
    signal shown(var toast)
    // The user asked for a toast's technical details; the main window shows them.
    signal detailsRequested(var toast)

    property int _nextId: 1

    // kind: "info" | "success" | "warning" | "danger". The optional action runs
    // ActionRegistry.trigger(actionId) when clicked. `details` (optional) is the technical text
    // behind a human message (an error from a library, a path, a code): without another
    // action, the toast offers a "Details" button that shows it (requestDetails).
    function show(text, kind, actionText, actionId, details) {
        const technical = details || "";
        const offerDetails = technical.length > 0 && !actionText;
        const toast = {
            id: _nextId++,
            text: text,
            kind: kind || "info",
            actionText: offerDetails ? qsTr("Details") : actionText || "",
            actionId: actionId || "",
            details: technical,
            time: new Date()
        };
        history = [toast].concat(history).slice(0, 100);
        unread += 1;
        shown(toast);
        return toast.id;
    }

    // Shows the technical details of toast `id` (its "Details" button).
    function requestDetails(id) {
        const toast = history.find(entry => entry.id === id);
        if (toast)
            detailsRequested(toast);
    }

    // What a toast's action button does: its action, or its details.
    function runAction(toast) {
        if (toast.actionId && toast.actionId.length > 0)
            ActionRegistry.trigger(toast.actionId);
        else if (toast.details && toast.details.length > 0)
            requestDetails(toast.id);
    }

    function markAllRead() {
        unread = 0;
    }

    function clearHistory() {
        history = [];
        unread = 0;
    }
}
