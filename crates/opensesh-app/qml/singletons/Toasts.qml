pragma Singleton

// In-app notifications: transient toasts plus the history shown in the notifications panel.
import QtQuick

QtObject {
    id: toasts

    // Newest first: [{ id, text, kind, actionText, actionId, time }].
    property var history: []
    property int unread: 0

    // Emitted for every new toast; the shell's toast host shows it.
    signal shown(var toast)

    property int _nextId: 1

    // kind: "info" | "success" | "warning" | "danger". The optional action runs
    // ActionRegistry.trigger(actionId) when clicked.
    function show(text, kind, actionText, actionId) {
        const toast = {
            id: _nextId++,
            text: text,
            kind: kind || "info",
            actionText: actionText || "",
            actionId: actionId || "",
            time: new Date()
        };
        history = [toast].concat(history).slice(0, 100);
        unread += 1;
        shown(toast);
        return toast.id;
    }

    function markAllRead() {
        unread = 0;
    }

    function clearHistory() {
        history = [];
        unread = 0;
    }
}
