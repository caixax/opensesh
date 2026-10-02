// Creates its popup the first time it is wanted (Sprint 17): a dialog that most sessions never
// open costs neither start-up time nor memory. Declare the popup as the `sourceComponent`.
// Functions: get() creates the popup if needed and returns it; close() closes it when it exists
// (without creating it).
import QtQuick

Loader {
    id: lazy

    function get() {
        active = true;
        return item;
    }

    function close() {
        if (item)
            item.close();
    }

    active: false
}
