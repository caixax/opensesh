#pragma once

namespace opensesh {

// Windows: makes OpenSesh's frameless windows (the custom title bar) ordinary resizable windows
// to the system again, so Aero Snap, Win+arrows, the minimize and maximize animations and the
// shadow work. Elsewhere it does nothing. Call once, after the QGuiApplication exists and before
// any window is created.
void install_window_frame_filter();

} // namespace opensesh
