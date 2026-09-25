// Settings (PLAN §5.4, §6.1). (Provisional stub: the settings pages are built in this sprint.)
//   section: string  selected section id, e.g. "general"
import QtQuick
import cc.caixa.opensesh

Item {
    id: view

    property string section: "general"

    function showSection(name) {
        section = name;
    }

    // Functions that visit every section, for SmokeTest.steps.
    function smokeSteps() {
        return [];
    }

    OsEmptyState {
        anchors.centerIn: parent
        iconName: "settings"
        title: qsTr("Settings")
    }
}
