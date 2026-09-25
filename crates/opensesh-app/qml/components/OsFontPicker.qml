// Font family chooser (see docs/design/components.md): an OsComboBox over the installed
// families (Platform.fontFamilies) where every item, and the closed box, is drawn in its own
// family. Typing jumps to the first family that starts with the typed text ("cons" ->
// Consolas); the typed text resets after a short pause.
//   monospaceOnly: bool     list fixed-pitch families only (terminal fonts)
//   currentFamily: string   the chosen family. Set it to preselect; it is updated when the user
//                           picks one (then `activated(index)` is emitted too). A family that is
//                           not installed is still shown as the current text.
import QtQuick
import cc.caixa.opensesh

OsComboBox {
    id: control

    property bool monospaceOnly: false
    property string currentFamily: ""

    function syncIndex() {
        currentIndex = currentFamily.length > 0 ? find(currentFamily) : -1;
    }

    // Highlights `index` in the open list, or selects it when the list is closed.
    function goTo(index: int) {
        if (popup.visible) {
            // highlightedIndex is read-only; these move it one step at a time.
            for (let i = highlightedIndex; i < index; ++i)
                incrementCurrentIndex();
            for (let i = highlightedIndex; i > index; --i)
                decrementCurrentIndex();
        } else if (index !== currentIndex) {
            currentIndex = index;
            activated(index);
        }
    }

    model: Platform.fontFamilies(monospaceOnly)
    previewFontFamilies: true
    displayText: currentIndex >= 0 ? currentText : currentFamily
    font.family: currentFamily.length > 0 ? currentFamily
                                          : monospaceOnly ? Theme.monoFontFamily : Theme.fontFamily

    Accessible.name: monospaceOnly ? qsTr("Monospace font") : qsTr("Font")
    Accessible.description: qsTr("Type the start of a font name to jump to it.")

    // Enter on an empty list activates -1: keep the family then.
    onActivated: index => {
        if (index >= 0)
            currentFamily = textAt(index);
    }
    onCurrentFamilyChanged: syncIndex()
    onCountChanged: syncIndex()
    Component.onCompleted: syncIndex()

    // Multi-letter type-ahead (the stock ComboBox only matches one letter at a time). Keys that
    // are not text, and Space when nothing has been typed yet (it opens the list), are left to
    // the ComboBox.
    Keys.onPressed: event => {
        const commandKeys = Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier;
        const typed = event.text;
        const code = typed.length > 0 ? typed.charCodeAt(0) : 0;
        if (code < 32 || code === 127 || (event.modifiers & commandKeys)
                || (event.key === Qt.Key_Space && typeAhead.typed.length === 0))
            return;
        event.accepted = true;
        const index = find(typeAhead.typed + typed, Qt.MatchStartsWith);
        typeAhead.restart();
        if (index < 0)
            return;
        typeAhead.typed += typed;
        goTo(index);
    }

    // Holds the text typed so far and clears it after a pause.
    Timer {
        id: typeAhead

        property string typed: ""

        interval: 1000
        onTriggered: typed = ""
    }

    // Opening or closing the list starts a new search.
    Connections {
        target: control.popup

        function onVisibleChanged() {
            typeAhead.stop();
            typeAhead.typed = "";
        }
    }
}
