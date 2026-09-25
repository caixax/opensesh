// Placeholder helper for features planned in a later sprint: shows an info toast such as
// "Quick connect is coming in Sprint 5." Declare one where it is needed:
//   ComingSoon { id: comingSoon }   ...   onClicked: comingSoon.notify(qsTr("Quick connect"), 5)
import QtQuick
import cc.caixa.opensesh

QtObject {
    // `feature` must already be translated.
    function notify(feature, sprint) {
        Toasts.show(qsTr("%1 is coming in Sprint %2.").arg(feature).arg(sprint), "info");
    }
}
