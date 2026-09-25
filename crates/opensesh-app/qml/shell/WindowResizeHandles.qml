// Invisible resize grips along the edges and corners of a frameless window. Dragging one calls
// window.startSystemResize(), so the window manager does the resizing (also on Wayland). Place
// it as the last child of the window content; it only takes input on its grips.
//   window: Window   the window to resize
//   grip: real       thickness of the edge grips (corners are twice as long)
import QtQuick
import cc.caixa.opensesh

Item {
    id: handles

    required property Window window
    property real grip: Math.round(Theme.spacingXs * 1.5)

    anchors.fill: parent
    z: 10000

    component Grip: Item {
        id: gripItem

        property int edges
        property int cursor: Qt.ArrowCursor
        // An inline component can't see the ids of this file; the grips are children of it.
        readonly property Window targetWindow: parent ? parent.window : null

        HoverHandler {
            cursorShape: gripItem.cursor
        }

        DragHandler {
            target: null
            onActiveChanged: {
                if (active && gripItem.targetWindow)
                    gripItem.targetWindow.startSystemResize(gripItem.edges);
            }
        }
    }

    Grip {
        x: handles.grip * 2
        width: parent.width - handles.grip * 4
        height: handles.grip
        edges: Qt.TopEdge
        cursor: Qt.SizeVerCursor
    }
    Grip {
        x: handles.grip * 2
        y: parent.height - height
        width: parent.width - handles.grip * 4
        height: handles.grip
        edges: Qt.BottomEdge
        cursor: Qt.SizeVerCursor
    }
    Grip {
        y: handles.grip * 2
        width: handles.grip
        height: parent.height - handles.grip * 4
        edges: Qt.LeftEdge
        cursor: Qt.SizeHorCursor
    }
    Grip {
        x: parent.width - width
        y: handles.grip * 2
        width: handles.grip
        height: parent.height - handles.grip * 4
        edges: Qt.RightEdge
        cursor: Qt.SizeHorCursor
    }

    // Corners: an L of two grips each, so they don't cover the content near the corner.
    Grip {
        width: handles.grip * 2
        height: handles.grip
        edges: Qt.TopEdge | Qt.LeftEdge
        cursor: Qt.SizeFDiagCursor
    }
    Grip {
        y: handles.grip
        width: handles.grip
        height: handles.grip
        edges: Qt.TopEdge | Qt.LeftEdge
        cursor: Qt.SizeFDiagCursor
    }
    Grip {
        x: parent.width - width
        width: handles.grip * 2
        height: handles.grip
        edges: Qt.TopEdge | Qt.RightEdge
        cursor: Qt.SizeBDiagCursor
    }
    Grip {
        x: parent.width - width
        y: handles.grip
        width: handles.grip
        height: handles.grip
        edges: Qt.TopEdge | Qt.RightEdge
        cursor: Qt.SizeBDiagCursor
    }
    Grip {
        y: parent.height - height
        width: handles.grip * 2
        height: handles.grip
        edges: Qt.BottomEdge | Qt.LeftEdge
        cursor: Qt.SizeBDiagCursor
    }
    Grip {
        y: parent.height - 2 * handles.grip
        width: handles.grip
        height: handles.grip
        edges: Qt.BottomEdge | Qt.LeftEdge
        cursor: Qt.SizeBDiagCursor
    }
    Grip {
        x: parent.width - width
        y: parent.height - height
        width: handles.grip * 2
        height: handles.grip
        edges: Qt.BottomEdge | Qt.RightEdge
        cursor: Qt.SizeFDiagCursor
    }
    Grip {
        x: parent.width - width
        y: parent.height - 2 * handles.grip
        width: handles.grip
        height: handles.grip
        edges: Qt.BottomEdge | Qt.RightEdge
        cursor: Qt.SizeFDiagCursor
    }
}
