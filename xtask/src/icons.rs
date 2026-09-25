//! `cargo xtask icons`: reproducible icon pipeline (PLAN §7).
//!
//! 1. Reads `assets/icons/icons.toml`.
//! 2. Downloads the pinned npm tarball of every referenced source (cached under
//!    `target/xtask-cache/`) and verifies its sha256 before touching it.
//! 3. Extracts only the listed icons. The only change applied is normalizing the root `stroke` /
//!    `fill` attributes to `currentColor` (and, for fill-style sources whose root has no `fill`,
//!    adding `fill="currentColor"`); path data is never modified.
//! 4. Writes them to `crates/opensesh-app/qml/icons/`, composes the placeholder application logo,
//!    copies the upstream licenses and regenerates `THIRD_PARTY_NOTICES.md`.
//!
//! The generated files are committed, so regular builds work offline.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::common::{
    fetch_verified, is_safe_file_name, is_safe_version, is_sha256_hex, write_if_changed,
};

/// Manifest location, relative to the workspace root.
const MANIFEST: &str = "assets/icons/icons.toml";
/// Output folder for UI icons, relative to the workspace root.
const ICONS_OUT: &str = "crates/opensesh-app/qml/icons";
/// Output folder for the application logo, relative to the workspace root.
const APP_ICON_OUT: &str = "crates/opensesh-app/data/icons";
/// Folder for upstream license texts, relative to the workspace root.
const LICENSES_OUT: &str = "assets/icons/LICENSES";
/// Upper bound for a downloaded tarball, to fail fast on unexpected responses.
const MAX_TARBALL_BYTES: u64 = 64 * 1024 * 1024;
/// Only SVG and license files at most this large are accepted from a tarball.
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    schema_version: u32,
    pub sources: BTreeMap<String, Source>,
    icons: BTreeMap<String, String>,
    app_icon: AppIcon,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub package: String,
    pub version: String,
    pub sha256: String,
    pub license: String,
    pub license_file: String,
    /// Extra upstream files shipped next to the license (for example a disclaimer), copied to
    /// `assets/icons/LICENSES/<source>-<file name>`.
    #[serde(default)]
    pub extra_notice_files: Vec<String>,
    /// Free text added to the source's entry in `THIRD_PARTY_NOTICES.md`.
    #[serde(default)]
    pub notice: Option<String>,
    icon_path: String,
    pub homepage: String,
    /// How the icons are painted, which decides the color normalization.
    #[serde(default)]
    style: Style,
}

/// How a source's icons are painted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Style {
    /// Outlines drawn with `stroke` (Lucide, Tabler outline). Root attributes are only rewritten.
    #[default]
    Stroke,
    /// Filled shapes (Simple Icons). A root without a `fill` attribute gets
    /// `fill="currentColor"`, since SVG would otherwise paint them black.
    Fill,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppIcon {
    glyph: String,
    background: String,
    foreground: String,
    size: u32,
    corner_radius: u32,
    padding: u32,
}

/// An icon reference such as `lucide:door-open`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IconRef<'a> {
    source: &'a str,
    name: &'a str,
}

fn parse_icon_ref(value: &str) -> Result<IconRef<'_>> {
    let Some((source, name)) = value.split_once(':') else {
        bail!("icon reference `{value}` must look like `<source>:<name>`");
    };
    ensure!(
        !source.is_empty() && is_safe_name(name),
        "icon reference `{value}` has an invalid source or name"
    );
    Ok(IconRef { source, name })
}

/// Icon and internal names may only use `[a-z0-9-]`, so they can't escape the output folder.
fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// npm package names, optionally scoped: `lucide-static`, `@tabler/icons`.
fn is_safe_package(package: &str) -> bool {
    let is_part = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_')
            })
    };
    match package.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| is_part(scope) && is_part(name)),
        None => is_part(package),
    }
}

/// Checks the source fields that end up in file names, URLs and the notices before any of them
/// is used: the id names the license copy, the version is part of the cache file and the URL.
fn validate_source(id: &str, source: &Source) -> Result<()> {
    ensure!(is_safe_name(id), "invalid source name `{id}`");
    ensure!(
        is_safe_package(&source.package),
        "source `{id}` has an invalid package name `{}`",
        source.package
    );
    ensure!(
        is_safe_version(&source.version),
        "source `{id}` has an invalid version `{}`",
        source.version
    );
    ensure!(
        is_sha256_hex(&source.sha256),
        "source `{id}` sha256 must be 64 lowercase hex characters"
    );
    for file in &source.extra_notice_files {
        ensure!(
            notice_file_name(file).is_some(),
            "source `{id}` has an invalid extra notice file `{file}`"
        );
    }
    Ok(())
}

/// Base name of an extra notice file (`package/DISCLAIMER.md` -> `DISCLAIMER.md`), if it is a
/// safe file name.
fn notice_file_name(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    (is_safe_file_name(name) && name != "LICENSE.txt").then_some(name)
}

/// Where a source's license copy lives, relative to the workspace root.
pub fn license_copy(source_id: &str) -> String {
    format!("{LICENSES_OUT}/{source_id}-LICENSE.txt")
}

/// Where a source's extra notice file lives, relative to the workspace root.
pub fn notice_copy(source_id: &str, upstream_path: &str) -> Option<String> {
    notice_file_name(upstream_path).map(|name| format!("{LICENSES_OUT}/{source_id}-{name}"))
}

/// Reads and validates `assets/icons/icons.toml`.
///
/// # Errors
///
/// Fails if the manifest can't be read or parsed, or has an invalid field or icon reference.
pub fn load_manifest(root: &Path) -> Result<Manifest> {
    let manifest_path = root.join(MANIFEST);
    let manifest: Manifest = toml::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    ensure!(
        manifest.schema_version == 1,
        "unsupported icons.toml schema_version {}",
        manifest.schema_version
    );
    for (id, source) in &manifest.sources {
        validate_source(id, source)?;
    }
    used_icons(&manifest)?;
    Ok(manifest)
}

/// The requested icons grouped by source, as (internal name, upstream name) pairs, so every
/// tarball is read once. Only sources with at least one icon are listed.
///
/// # Errors
///
/// Fails on an invalid internal name or icon reference, or an unknown source.
pub fn used_icons(manifest: &Manifest) -> Result<BTreeMap<&str, Vec<(&str, &str)>>> {
    let mut wanted: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (internal, reference) in &manifest.icons {
        ensure!(
            is_safe_name(internal),
            "invalid internal icon name `{internal}`"
        );
        let icon = parse_icon_ref(reference)?;
        ensure!(
            manifest.sources.contains_key(icon.source),
            "icon `{internal}` uses unknown source `{}`",
            icon.source
        );
        wanted
            .entry(icon.source)
            .or_default()
            .push((internal.as_str(), icon.name));
    }
    Ok(wanted)
}

/// Runs the whole pipeline from the workspace root.
///
/// # Errors
///
/// Fails on network errors, checksum mismatches, missing icons or I/O errors.
pub fn run(root: &Path) -> Result<()> {
    let manifest = load_manifest(root)?;
    let wanted = used_icons(&manifest)?;

    let icons_out = root.join(ICONS_OUT);
    let licenses_out = root.join(LICENSES_OUT);
    std::fs::create_dir_all(&icons_out)?;
    std::fs::create_dir_all(&licenses_out)?;

    let mut extracted: BTreeMap<String, String> = BTreeMap::new();
    for (source_id, icons) in &wanted {
        let source = manifest
            .sources
            .get(*source_id)
            .with_context(|| format!("unknown source `{source_id}`"))?;
        let tarball = fetch_tarball(root, source)?;
        let mut files = read_tarball(&tarball, source, icons)?;

        let license = files
            .remove(&source.license_file)
            .with_context(|| format!("{} not found in the tarball", source.license_file))?;
        write_if_changed(&root.join(license_copy(source_id)), &license)?;
        for upstream in &source.extra_notice_files {
            let text = files
                .remove(upstream)
                .with_context(|| format!("{upstream} not found in the tarball"))?;
            let copy = notice_copy(source_id, upstream)
                .with_context(|| format!("invalid extra notice file `{upstream}`"))?;
            write_if_changed(&root.join(copy), &text)?;
        }

        for (internal, upstream) in icons {
            let path = source.icon_path.replace("{name}", upstream);
            let svg = files
                .remove(&path)
                .with_context(|| format!("icon `{source_id}:{upstream}` ({path}) not found"))?;
            let normalized = normalize_root_colors(&svg, source.style)?;
            write_if_changed(&icons_out.join(format!("{internal}.svg")), &normalized)?;
            extracted.insert((*internal).to_owned(), normalized);
            println!("icon {internal:<24} <- {source_id}:{upstream}");
        }
    }
    remove_stale_icons(&icons_out, &extracted)?;

    let glyph = extracted.get(&manifest.app_icon.glyph).with_context(|| {
        format!(
            "app_icon.glyph `{}` must be listed in [icons]",
            manifest.app_icon.glyph
        )
    })?;
    let logo = compose_app_icon(glyph, &manifest.app_icon)?;
    let app_icon_out = root.join(APP_ICON_OUT);
    std::fs::create_dir_all(&app_icon_out)?;
    write_if_changed(
        &app_icon_out.join(format!("{}.svg", opensesh_core::identity::APP_ID)),
        &logo,
    )?;
    println!(
        "app icon {APP_ICON_OUT}/{}.svg",
        opensesh_core::identity::APP_ID
    );

    crate::notices::write(root)
}

/// Returns the verified tarball bytes, downloading them unless a verified cached copy exists.
fn fetch_tarball(root: &Path, source: &Source) -> Result<Vec<u8>> {
    let cache_name = format!(
        "{}-{}.tgz",
        source.package.replace('/', "__"),
        source.version
    );
    fetch_verified(
        root,
        &cache_name,
        &tarball_url(&source.package, &source.version),
        &source.sha256,
        MAX_TARBALL_BYTES,
    )
}

/// npm registry tarball URL. Scoped packages (`@scope/name`) keep the scope in the path but not
/// in the file name.
fn tarball_url(package: &str, version: &str) -> String {
    let base_name = package.rsplit('/').next().unwrap_or(package);
    format!("https://registry.npmjs.org/{package}/-/{base_name}-{version}.tgz")
}

/// Reads the license and notice files and the requested icons out of a `.tgz`, as UTF-8 text.
fn read_tarball(
    tarball: &[u8],
    source: &Source,
    icons: &[(&str, &str)],
) -> Result<BTreeMap<String, String>> {
    let mut wanted: Vec<String> = icons
        .iter()
        .map(|(_, upstream)| source.icon_path.replace("{name}", upstream))
        .collect();
    wanted.push(source.license_file.clone());
    wanted.extend(source.extra_notice_files.iter().cloned());

    let mut found = BTreeMap::new();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
    for entry in archive.entries().context("reading tarball")? {
        let mut entry = entry.context("reading tarball entry")?;
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if !wanted.contains(&path) {
            continue;
        }
        ensure!(
            entry.header().entry_type().is_file(),
            "{path} is not a regular file"
        );
        ensure!(
            entry.size() <= MAX_FILE_BYTES,
            "{path} is unexpectedly large"
        );
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .with_context(|| format!("{path} is not UTF-8 text"))?;
        found.insert(path, text);
    }
    Ok(found)
}

/// Sets the root `<svg>` element's `stroke` / `fill` to `currentColor` unless they are `none`.
/// For [`Style::Fill`] sources, a root without a `fill` attribute also gets
/// `fill="currentColor"`. Nothing else in the file is touched, so child elements (and their own
/// `fill` / `stroke`, such as Tabler's invisible `stroke="none"` bounding box) stay verbatim.
fn normalize_root_colors(svg: &str, style: Style) -> Result<String> {
    let start = svg.find("<svg").context("no <svg> element")?;
    let end = start + svg[start..].find('>').context("unterminated <svg> tag")?;
    let tag = &svg[start..end];

    let mut normalized_tag = String::with_capacity(tag.len() + 24);
    let mut has_fill = false;
    let mut rest = tag;
    while let Some(pos) = find_color_attribute(rest) {
        let (before, attr) = rest.split_at(pos);
        normalized_tag.push_str(before);
        has_fill |= attr.starts_with("fill=");
        let name_end = attr.find('=').context("malformed attribute")?;
        let quote = attr[name_end + 1..]
            .chars()
            .next()
            .context("malformed attribute")?;
        let value_start = name_end + 2;
        let value_len = attr[value_start..]
            .find(quote)
            .context("unterminated attribute")?;
        let value = &attr[value_start..value_start + value_len];
        normalized_tag.push_str(&attr[..value_start]);
        normalized_tag.push_str(if value == "none" {
            "none"
        } else {
            "currentColor"
        });
        rest = &attr[value_start + value_len..];
    }
    normalized_tag.push_str(rest);

    if style == Style::Fill && !has_fill {
        // Insert before a self-closing `/` and any trailing whitespace, so a multi-line root tag
        // keeps its closing `>` on its own line.
        let body = normalized_tag.trim_end();
        let body = body.strip_suffix('/').map_or(body, str::trim_end);
        let insert_at = body.len();
        normalized_tag.insert_str(insert_at, r#" fill="currentColor""#);
    }

    Ok(format!("{}{normalized_tag}{}", &svg[..start], &svg[end..]))
}

/// Finds the next ` stroke=` or ` fill=` attribute (preceded by whitespace) in a tag.
fn find_color_attribute(tag: &str) -> Option<usize> {
    let bytes = tag.as_bytes();
    (1..bytes.len()).find(|&i| {
        bytes[i - 1].is_ascii_whitespace()
            && (tag[i..].starts_with("stroke=\"")
                || tag[i..].starts_with("stroke='")
                || tag[i..].starts_with("fill=\"")
                || tag[i..].starts_with("fill='"))
    })
}

/// Builds the placeholder logo: a rounded square (a `<rect>` primitive computed from the
/// manifest numbers, no path data) with the untouched glyph children on top.
fn compose_app_icon(glyph_svg: &str, spec: &AppIcon) -> Result<String> {
    ensure!(
        is_hex_color(&spec.background) && is_hex_color(&spec.foreground),
        "app_icon colors must be #RRGGBB"
    );
    // Checked, so an absurd padding is an error rather than an overflow.
    let glyph_size = spec
        .padding
        .checked_mul(2)
        .and_then(|margins| spec.size.checked_sub(margins))
        .filter(|size| *size > 0)
        .context("app_icon padding is too large")?;
    let open_start = glyph_svg.find("<svg").context("no <svg> element")?;
    let body_start = open_start
        + glyph_svg[open_start..]
            .find('>')
            .context("unterminated <svg>")?
        + 1;
    let body_end = glyph_svg.rfind("</svg>").context("no </svg>")?;
    ensure!(
        body_end >= body_start,
        "</svg> must come after the opening <svg> tag"
    );
    let body = glyph_svg[body_start..body_end].trim_matches('\n');

    // Lucide glyphs are drawn on a 24x24 grid.
    let scale = f64::from(glyph_size) / 24.0;
    let mut out = String::new();
    writeln!(
        out,
        "<!-- Placeholder OpenSesh logo generated by `cargo xtask icons` from assets/icons/icons.toml. -->"
    )?;
    writeln!(
        out,
        "<!-- Glyph: Lucide `{}` (ISC, see assets/icons/LICENSES/lucide-LICENSE.txt), paths unmodified. -->",
        spec.glyph
    )?;
    writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">"#,
        s = spec.size
    )?;
    writeln!(
        out,
        r#"  <rect x="0" y="0" width="{s}" height="{s}" rx="{r}" ry="{r}" fill="{bg}"/>"#,
        s = spec.size,
        r = spec.corner_radius,
        bg = spec.background
    )?;
    writeln!(
        out,
        r#"  <g transform="translate({p} {p}) scale({scale:.6})" fill="none" stroke="{fg}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
        p = spec.padding,
        fg = spec.foreground
    )?;
    for line in body.lines() {
        writeln!(out, "  {line}")?;
    }
    writeln!(out, "  </g>")?;
    writeln!(out, "</svg>")?;
    Ok(out)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Deletes generated icons that are no longer listed in the manifest.
fn remove_stale_icons(dir: &Path, keep: &BTreeMap<String, String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_svg = path.extension().is_some_and(|ext| ext == "svg");
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        if is_svg && stem.is_some_and(|stem| !keep.contains_key(&stem)) {
            std::fs::remove_file(&path)?;
            println!("removed stale {}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOOR_OPEN: &str = r#"<!-- @license lucide-static v1.48.0 - ISC -->
<svg
  class="lucide lucide-door-open"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M10 21H2" />
  <path d="M22 21h-3" />
</svg>
"#;

    /// Shape of every Simple Icons file: no `fill` anywhere.
    const DEBIAN: &str = r#"<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>Debian</title><path d="M13.88 12.685c-.4 0 .08.2.601.28"/></svg>"#;

    /// Shape of every Tabler outline file: an invisible bounding-box path first.
    const TABLER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon icon-tabler icons-tabler-outline icon-tabler-brand-windows"><path stroke="none" d="M0 0h24v24H0z" fill="none"/><path d="M17.8 20l-12 -1.5" /></svg>"#;

    #[test]
    fn icon_refs_are_parsed_and_validated() {
        assert_eq!(
            parse_icon_ref("lucide:door-open").unwrap(),
            IconRef {
                source: "lucide",
                name: "door-open"
            }
        );
        assert!(parse_icon_ref("door-open").is_err());
        assert!(parse_icon_ref("lucide:../etc/passwd").is_err());
        assert!(parse_icon_ref("lucide:Door").is_err());
        assert!(parse_icon_ref(":x").is_err());
    }

    #[test]
    fn scoped_packages_use_the_bare_name_in_the_tarball_file() {
        assert_eq!(
            tarball_url("lucide-static", "1.48.0"),
            "https://registry.npmjs.org/lucide-static/-/lucide-static-1.48.0.tgz"
        );
        assert_eq!(
            tarball_url("@tabler/icons", "3.48.0"),
            "https://registry.npmjs.org/@tabler/icons/-/icons-3.48.0.tgz"
        );
    }

    #[test]
    fn package_names_are_restricted() {
        assert!(is_safe_package("lucide-static"));
        assert!(is_safe_package("simple-icons"));
        assert!(is_safe_package("@tabler/icons"));
        assert!(!is_safe_package(""));
        assert!(!is_safe_package("@tabler"));
        assert!(!is_safe_package("@/icons"));
        assert!(!is_safe_package("@tabler/../x"));
        assert!(!is_safe_package("a/b"));
        assert!(!is_safe_package("Lucide"));
        assert!(!is_safe_package("lucide?x=1"));
    }

    #[test]
    fn lucide_icons_are_already_normalized() {
        assert_eq!(
            normalize_root_colors(DOOR_OPEN, Style::Stroke).unwrap(),
            DOOR_OPEN
        );
    }

    #[test]
    fn tabler_outline_icons_are_already_normalized() {
        assert_eq!(
            normalize_root_colors(TABLER, Style::Stroke).unwrap(),
            TABLER
        );
    }

    #[test]
    fn root_colors_are_normalized_without_touching_children() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" fill="#000" stroke='red'><path fill="#fff" d="M0 0"/></svg>"##;
        let expected = r##"<svg xmlns="http://www.w3.org/2000/svg" fill="currentColor" stroke='currentColor'><path fill="#fff" d="M0 0"/></svg>"##;
        assert_eq!(normalize_root_colors(svg, Style::Stroke).unwrap(), expected);
        // An existing root fill is rewritten, never duplicated.
        assert_eq!(normalize_root_colors(svg, Style::Fill).unwrap(), expected);
        let none = r#"<svg fill="none" stroke-width="2"></svg>"#;
        assert_eq!(normalize_root_colors(none, Style::Stroke).unwrap(), none);
        assert_eq!(normalize_root_colors(none, Style::Fill).unwrap(), none);
    }

    #[test]
    fn fill_sources_get_a_current_color_root_fill() {
        let normalized = normalize_root_colors(DEBIAN, Style::Fill).unwrap();
        assert_eq!(
            normalized,
            r#"<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><title>Debian</title><path d="M13.88 12.685c-.4 0 .08.2.601.28"/></svg>"#
        );
        // Path data and children are untouched: only the root tag grew.
        let root_end = DEBIAN.find('>').unwrap();
        assert!(normalized.ends_with(&DEBIAN[root_end..]));
        // Idempotent: a second run finds the fill it added.
        assert_eq!(
            normalize_root_colors(&normalized, Style::Fill).unwrap(),
            normalized
        );
        // Stroke sources never get a fill added.
        assert_eq!(
            normalize_root_colors(DEBIAN, Style::Stroke).unwrap(),
            DEBIAN
        );
    }

    #[test]
    fn fill_is_added_before_a_self_closing_slash_and_trailing_whitespace() {
        assert_eq!(
            normalize_root_colors(r#"<svg viewBox="0 0 24 24" />"#, Style::Fill).unwrap(),
            r#"<svg viewBox="0 0 24 24" fill="currentColor" />"#
        );
        assert_eq!(
            normalize_root_colors(
                "<svg\n  viewBox=\"0 0 24 24\"\n>\n<path d=\"M0 0\"/></svg>",
                Style::Fill
            )
            .unwrap(),
            "<svg\n  viewBox=\"0 0 24 24\" fill=\"currentColor\"\n>\n<path d=\"M0 0\"/></svg>"
        );
        // An attribute that merely ends in `fill` is not a fill.
        assert_eq!(
            normalize_root_colors(r#"<svg data-fill="x"></svg>"#, Style::Fill).unwrap(),
            r#"<svg data-fill="x" fill="currentColor"></svg>"#
        );
    }

    #[test]
    fn app_icon_keeps_glyph_paths_verbatim() {
        let spec = AppIcon {
            glyph: "door-open".into(),
            background: "#E6B450".into(),
            foreground: "#FFFFFF".into(),
            size: 256,
            corner_radius: 56,
            padding: 48,
        };
        let logo = compose_app_icon(DOOR_OPEN, &spec).unwrap();
        assert!(logo.contains(r#"<path d="M10 21H2" />"#));
        assert!(logo.contains(r#"<path d="M22 21h-3" />"#));
        assert!(logo.contains(r##"rx="56" ry="56" fill="#E6B450""##));
        assert!(logo.contains(r#"scale(6.666667)"#));
        assert!(logo.contains(r##"stroke="#FFFFFF""##));
        assert!(!logo.contains("currentColor"));
    }

    #[test]
    fn app_icon_rejects_bad_specs() {
        let mut spec = AppIcon {
            glyph: "door-open".into(),
            background: "amber".into(),
            foreground: "#FFFFFF".into(),
            size: 256,
            corner_radius: 56,
            padding: 48,
        };
        assert!(compose_app_icon(DOOR_OPEN, &spec).is_err());
        spec.background = "#E6B450".into();
        spec.padding = 128;
        assert!(compose_app_icon(DOOR_OPEN, &spec).is_err());
        // `padding * 2` would overflow a u32.
        spec.padding = u32::MAX / 2 + 1;
        assert!(compose_app_icon(DOOR_OPEN, &spec).is_err());
        spec.padding = u32::MAX;
        assert!(compose_app_icon(DOOR_OPEN, &spec).is_err());
    }

    #[test]
    fn app_icon_rejects_a_closing_tag_before_the_opening_one() {
        let spec = AppIcon {
            glyph: "door-open".into(),
            background: "#E6B450".into(),
            foreground: "#FFFFFF".into(),
            size: 256,
            corner_radius: 56,
            padding: 48,
        };
        assert!(compose_app_icon("</svg><svg>", &spec).is_err());
        // The only `</svg>` sits inside the unterminated opening tag.
        assert!(compose_app_icon("<svg </svg>", &spec).is_err());
        assert!(compose_app_icon("<svg></svg>", &spec).is_ok());
    }

    fn source(version: &str, sha256: &str) -> Source {
        Source {
            package: "lucide-static".into(),
            version: version.into(),
            sha256: sha256.into(),
            license: "ISC".into(),
            license_file: "package/LICENSE".into(),
            extra_notice_files: Vec::new(),
            notice: None,
            icon_path: "package/icons/{name}.svg".into(),
            homepage: "https://lucide.dev".into(),
            style: Style::Stroke,
        }
    }

    const SHA: &str = "3c2ecda3d25f6a9692d83f8036d9a526f7da584a51af74cd16eda4498c5c33d8";

    #[test]
    fn source_ids_must_be_safe_names() {
        let valid = source("1.48.0", SHA);
        assert!(validate_source("lucide", &valid).is_ok());
        assert!(validate_source("", &valid).is_err());
        assert!(validate_source("../lucide", &valid).is_err());
        assert!(validate_source("a/b", &valid).is_err());
        assert!(validate_source("a\\b", &valid).is_err());
        assert!(validate_source("Lucide", &valid).is_err());
    }

    #[test]
    fn source_versions_are_restricted() {
        assert!(validate_source("lucide", &source("2.0.0-rc.1+build.5", SHA)).is_ok());
        assert!(validate_source("lucide", &source("", SHA)).is_err());
        assert!(validate_source("lucide", &source("1.0.0/../../x", SHA)).is_err());
        assert!(validate_source("lucide", &source("1.0.0?x=1", SHA)).is_err());
    }

    #[test]
    fn source_checksums_must_be_lowercase_sha256_hex() {
        assert!(validate_source("lucide", &source("1.48.0", &SHA.to_uppercase())).is_err());
        assert!(validate_source("lucide", &source("1.48.0", &SHA[..63])).is_err());
        assert!(validate_source("lucide", &source("1.48.0", "")).is_err());
    }

    #[test]
    fn extra_notice_files_must_have_safe_names() {
        let mut simple = source("16.32.0", SHA);
        simple.extra_notice_files = vec!["package/DISCLAIMER.md".into()];
        assert!(validate_source("simple", &simple).is_ok());
        assert_eq!(
            notice_copy("simple", "package/DISCLAIMER.md").as_deref(),
            Some("assets/icons/LICENSES/simple-DISCLAIMER.md")
        );
        for bad in ["package/", "package/..", "package/.hidden", "LICENSE.txt"] {
            simple.extra_notice_files = vec![bad.into()];
            assert!(validate_source("simple", &simple).is_err(), "{bad}");
        }
    }

    #[test]
    fn manifest_in_repo_parses() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifest = load_manifest(&root).unwrap();
        let used = used_icons(&manifest).unwrap();
        for (source_id, icons) in &used {
            let source = &manifest.sources[*source_id];
            assert_eq!(source.sha256.len(), 64);
            assert!(!icons.is_empty());
        }
        assert!(manifest.icons.contains_key(&manifest.app_icon.glyph));
        // Simple Icons files have no fill, so they must be normalized as a fill source.
        if let Some(simple) = manifest.sources.get("simple") {
            assert_eq!(simple.style, Style::Fill);
        }
    }
}
