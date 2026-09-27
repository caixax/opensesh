pragma Singleton

// Files copied or cut in a file pane (Sprint 8), until they are pasted in any pane of any window.
// Pasting cut files in the pane they come from moves them there; anywhere else it transfers them
// (and deletes the sources once copied).
//   pane: int         the SftpBrowser.paneId they come from (0: none)
//   paths: var        their paths
//   folder: string    the folder they are in
//   cut: bool         a move rather than a copy
// Functions: set(pane, paths, folder, cut), clear().
import QtQuick

QtObject {
    property int pane: 0
    property var paths: []
    property string folder: ""
    property bool cut: false
    readonly property bool empty: pane === 0 || paths.length === 0

    function set(from, list, where, moving) {
        pane = from;
        paths = list;
        folder = where;
        cut = moving;
    }

    function clear() {
        pane = 0;
        paths = [];
        folder = "";
        cut = false;
    }
}
