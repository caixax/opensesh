//! Builds the cxx-qt bridges, the C++ shim and the `cc.caixa.opensesh` QML module.
//!
//! Files are discovered, so adding a component, bridge or asset needs no edit here:
//! - `qml/**/*.qml` are QML module files; those under `qml/singletons/` are QML singletons;
//! - `src/bridge/*.rs` (except `mod.rs`) are cxx-qt bridges;
//! - `cpp/*.cpp` are compiled; `cpp/*.h` are exported as `opensesh-app/<name>.h` and run through
//!   moc, so a header can declare `Q_OBJECT` classes, and `QML_ELEMENT` / `QML_ANONYMOUS` types
//!   that qmltyperegistrar adds to the QML module (a header without `Q_OBJECT` gets an empty moc
//!   file);
//! - `qml/icons/*.svg`, `fonts/*.{ttf,otf}`, `i18n/*.qm`, `data/icons/*.svg` and `shaders/*.qsb`
//!   (compiled by `cargo xtask shaders`, ADR 0013) go into the Qt resources under
//!   `qrc:/qt/qml/cc/caixa/opensesh/`.

use std::path::{Path, PathBuf};

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    for watched in [
        "qml",
        "src/bridge",
        "cpp",
        "fonts",
        "i18n",
        "data",
        "shaders",
    ] {
        println!("cargo::rerun-if-changed={watched}");
    }

    let qml_files: Vec<QmlFile> = files_with_extensions(Path::new("qml"), &["qml"], true)
        .into_iter()
        .map(|path| {
            let singleton = path.starts_with("qml/singletons/");
            QmlFile::from(path).singleton(singleton)
        })
        .collect();
    let bridges: Vec<String> = files_with_extensions(Path::new("src/bridge"), &["rs"], false)
        .into_iter()
        .filter(|path| !path.ends_with("/mod.rs"))
        .collect();
    // Sources are compiled; headers are moc'd (cxx-qt-build tells them apart by extension).
    let cpp_files = files_with_extensions(Path::new("cpp"), &["cpp", "h"], false);

    let mut resources = Vec::new();
    resources.extend(files_with_extensions(
        Path::new("qml/icons"),
        &["svg"],
        false,
    ));
    resources.extend(files_with_extensions(
        Path::new("data/icons"),
        &["svg"],
        false,
    ));
    resources.extend(files_with_extensions(
        Path::new("fonts"),
        &["ttf", "otf"],
        false,
    ));
    // Qt Quick material shaders (terminal glyphs), loaded by QSGMaterialShader.
    resources.extend(files_with_extensions(Path::new("shaders"), &["qsb"], false));
    // The pseudo-locale is a debug tool (ADR 0009): release builds don't bundle it, so a
    // `language = "pseudo"` left in a shared config.toml falls back to English there.
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    resources.extend(
        files_with_extensions(Path::new("i18n"), &["qm"], false)
            .into_iter()
            .filter(|file| !(release && file.ends_with("opensesh_pseudo.qm"))),
    );

    let module = QmlModule::new("cc.caixa.opensesh")
        .version(1, 0)
        .qml_files(qml_files)
        // Lets qmllint / qmlls resolve the QtQuick types used by the module.
        .depend("QtQuick");

    let builder = CxxQtBuilder::new_qml_module(module)
        // Export only cpp/ as `opensesh-app/...` headers. The default (the whole crate) makes
        // cxx-qt-build watch every file in the crate, so any Rust edit would rerun moc,
        // qmlcachegen, rcc and all C++ compilation.
        .crate_include_root(Some("cpp".to_owned()))
        .files(bridges)
        .cpp_files(cpp_files)
        .qrc_resources(resources)
        // QQuickImageProvider (icons), the terminal's scene graph nodes, and QSvgRenderer.
        .qt_module("Quick")
        .qt_module("Svg")
        // Qt Qml requires Qt Network on macOS; linking it everywhere keeps the build uniform.
        .qt_module("Network");

    // qmlcachegen compiles QML bindings to C++ that embeds the UTF-8 QML strings. Without
    // `/utf-8`, MSVC reads those sources in the ANSI code page and mangles any non-ASCII text
    // ("·" became "Â·"). Qt's own CMake integration passes the same flag.
    let is_msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    // SAFETY: only adds a source-encoding flag; it doesn't change the ABI, defines or any other
    // setting cxx-qt-build relies on.
    let builder = unsafe {
        builder.cc_builder(move |cc| {
            if is_msvc {
                cc.flag("/utf-8");
            }
        })
    };
    builder.build();

    add_qt_rpath_for_private_installs();
}

/// Sorted paths (forward slashes, relative to the crate) of the files in `dir` whose extension
/// is one of `extensions`; recursive if asked. Empty if `dir` doesn't exist.
fn files_with_extensions(dir: &Path, extensions: &[&str], recursive: bool) -> Vec<String> {
    let mut out = Vec::new();
    collect(dir, extensions, recursive, &mut out);
    let mut files: Vec<String> = out
        .into_iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    files.sort();
    files
}

fn collect(dir: &Path, extensions: &[&str], recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            if recursive {
                collect(&path, extensions, recursive, out);
            }
        } else if path
            .extension()
            .is_some_and(|ext| extensions.iter().any(|wanted| ext == *wanted))
        {
            out.push(path);
        }
    }
}

/// On Linux, a Qt installed outside the dynamic loader's default directories (aqtinstall, the Qt
/// online installer, a source build in `/usr/local/Qt-x.y.z`) can't be found at run time, so
/// `cargo run` / `cargo test` would fail with "libQt6Gui.so.6: cannot open shared object file".
/// Embed an rpath to its `lib` directory in that case. Distro Qt needs nothing, and release
/// packaging (Sprint 18) rewrites the rpath anyway.
fn add_qt_rpath_for_private_installs() {
    println!("cargo::rerun-if-env-changed=QMAKE");
    let is_linux = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "linux");
    if !is_linux {
        return;
    }
    let Some(libs) = qmake_query("QT_INSTALL_LIBS") else {
        return;
    };
    if !is_loader_default_dir(&libs) {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{libs}");
    }
}

/// Directories the Linux dynamic loader searches without configuration, including multiarch
/// subdirectories such as `/usr/lib/x86_64-linux-gnu`.
fn is_loader_default_dir(dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    [
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib32",
        "/lib",
        "/lib64",
        "/lib32",
    ]
    .iter()
    .any(|default| {
        dir == *default
            || dir.strip_prefix(default).is_some_and(|rest| {
                rest.starts_with('/')
                    && rest.ends_with("-linux-gnu")
                    && rest.matches('/').count() == 1
            })
    })
}

/// Runs `qmake -query <property>` with the same qmake lookup as cxx-qt's `qt-build-utils`:
/// the `QMAKE` environment variable first, then `qmake6` / `qmake` on `PATH`.
fn qmake_query(property: &str) -> Option<String> {
    let candidates: Vec<String> = match std::env::var("QMAKE") {
        Ok(qmake) if !qmake.is_empty() => vec![qmake],
        _ => vec!["qmake6".to_owned(), "qmake".to_owned()],
    };
    candidates.iter().find_map(|qmake| {
        let output = std::process::Command::new(qmake)
            .args(["-query", property])
            .output()
            .ok()?;
        let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (output.status.success() && !value.is_empty()).then_some(value)
    })
}
