pragma ComponentBehavior: Bound

// Shows the toasts announced by the `Toasts` singleton (`Toasts.show(...)`), stacked in the
// bottom-right corner of its parent with the newest at the bottom. Put one in each window, as
// the last child of the window content so it stays on top. Toasts dismiss themselves after
// `timeout` unless the pointer is over them or one of their buttons has keyboard focus; the
// action button runs `ActionRegistry.trigger(actionId)` and then dismisses the toast. Screen
// readers get each new message through Accessible.announce().
//   maxVisible: int   toasts shown at once (default 4); the oldest is dropped first
//   timeout: int      auto-dismiss delay in ms (default 5000); 0 keeps toasts until closed
//   count: int        read-only; toasts currently shown
// Functions: show(toast) (a `Toasts.shown` payload), dismiss(toastId), clear().
import QtQuick
import cc.caixa.opensesh

Item {
    id: host

    property int maxVisible: 4
    property int timeout: 5000
    readonly property int count: toastModel.count

    function show(toast) {
        toastModel.insert(0, {
            toastId: toast.id,
            message: toast.text || "",
            toastKind: toast.kind || "info",
            toastActionText: toast.actionText || "",
            toastActionId: toast.actionId || ""
        });
        while (toastModel.count > Math.max(1, maxVisible))
            toastModel.remove(toastModel.count - 1);
    }

    function dismiss(toastId) {
        for (let i = 0; i < toastModel.count; ++i) {
            if (toastModel.get(i).toastId === toastId) {
                toastModel.remove(i);
                return;
            }
        }
    }

    function clear() {
        toastModel.clear();
    }

    anchors.right: parent ? parent.right : undefined
    anchors.bottom: parent ? parent.bottom : undefined
    anchors.margins: Theme.spacingLg
    // Tall enough for the stack; the empty area lets every pointer event through.
    width: Math.min(Theme.spacingXs * 105, parent ? parent.width - 2 * Theme.spacingLg : 0)
    height: parent ? parent.height - 2 * Theme.spacingLg : 0
    z: 1000

    Accessible.ignored: toastModel.count === 0

    ListModel {
        id: toastModel
    }

    Connections {
        target: Toasts

        function onShown(toast) {
            host.show(toast);
        }
    }

    ListView {
        id: list

        anchors.fill: parent
        verticalLayoutDirection: ListView.BottomToTop
        interactive: false
        spacing: Theme.spacingSm
        model: toastModel

        // The view positions its delegates, so a full-width row right-aligns the card.
        delegate: Item {
            id: row

            required property int toastId
            required property string message
            required property string toastKind
            required property string toastActionText
            required property string toastActionId

            // Horizontal slide used by the add and remove transitions.
            property real slide: 0

            width: ListView.view.width
            height: toastItem.height
            transform: Translate {
                x: row.slide
            }

            OsToast {
                id: toastItem

                anchors.right: parent.right
                width: implicitWidth
                maxWidth: row.width
                kind: row.toastKind
                text: row.message
                actionText: row.toastActionText

                onActionClicked: {
                    if (row.toastActionId.length > 0)
                        ActionRegistry.trigger(row.toastActionId);
                    host.dismiss(row.toastId);
                }
                onCloseClicked: host.dismiss(row.toastId)

                Component.onCompleted: Accessible.announce(text)
            }

            Timer {
                interval: host.timeout
                running: host.timeout > 0 && !toastItem.hovered && !toastItem.focusInside
                onTriggered: host.dismiss(row.toastId)
            }
        }

        add: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: Theme.durationNormal
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                property: "slide"
                from: Theme.spacingXxl
                to: 0
                duration: Theme.durationNormal
                easing.type: Easing.OutCubic
            }
        }

        remove: Transition {
            NumberAnimation {
                property: "opacity"
                to: 0
                duration: Theme.durationNormal
                easing.type: Easing.InCubic
            }
            NumberAnimation {
                property: "slide"
                to: Theme.spacingXxl
                duration: Theme.durationNormal
                easing.type: Easing.InCubic
            }
        }

        // Also restores opacity and slide if an add transition was interrupted.
        displaced: Transition {
            NumberAnimation {
                property: "y"
                duration: Theme.durationNormal
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                property: "opacity"
                to: 1
                duration: Theme.durationNormal
            }
            NumberAnimation {
                property: "slide"
                to: 0
                duration: Theme.durationNormal
            }
        }
    }
}
