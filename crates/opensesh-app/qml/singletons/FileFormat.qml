pragma Singleton

// How file sizes, speeds, durations and times read in the file panes and the transfer queue.
// Functions: size(bytes), speed(bytesPerSecond), duration(seconds), time(secondsSinceEpoch),
// fileUrl(localPath).
import QtQuick

QtObject {
    // "0 B", "912 B", "1.2 KB", "3.4 MB", "1.07 GB" (powers of 1024).
    function size(bytes) {
        if (bytes === undefined || bytes === null || bytes < 0)
            return "";
        if (bytes < 1024)
            return qsTr("%1 B").arg(bytes);
        const units = [qsTr("%1 KB"), qsTr("%1 MB"), qsTr("%1 GB"), qsTr("%1 TB")];
        let value = bytes / 1024;
        let unit = 0;
        while (value >= 1024 && unit < units.length - 1) {
            value /= 1024;
            unit += 1;
        }
        const digits = value < 10 ? 2 : value < 100 ? 1 : 0;
        return units[unit].arg(Number(value.toFixed(digits)).toLocaleString(Qt.locale(), "f", digits));
    }

    function speed(bytesPerSecond) {
        return bytesPerSecond > 0 ? qsTr("%1/s").arg(size(Math.round(bytesPerSecond))) : "";
    }

    // "42 s", "3 min 5 s", "1 h 20 min".
    function duration(seconds) {
        if (seconds === undefined || seconds === null || seconds < 0)
            return "";
        if (seconds < 60)
            return qsTr("%1 s").arg(seconds);
        if (seconds < 3600)
            return qsTr("%1 min %2 s").arg(Math.floor(seconds / 60)).arg(seconds % 60);
        return qsTr("%1 h %2 min").arg(Math.floor(seconds / 3600)).arg(Math.floor(seconds % 3600 / 60));
    }

    // The locale's short date and time; empty when unknown.
    function time(seconds) {
        if (seconds === undefined || seconds === null || seconds < 0)
            return "";
        return new Date(seconds * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat);
    }

    // A file:// URL for a local path (Windows or Unix).
    function fileUrl(path) {
        const slashed = path.replace(/\\/g, "/");
        return "file://" + (slashed.startsWith("/") ? "" : "/") + encodeURI(slashed);
    }
}
