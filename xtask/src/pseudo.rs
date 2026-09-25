//! Pseudo-localization of lupdate `.ts` files. The pseudo-locale exercises the whole i18n
//! pipeline and shows untranslated strings (plain ASCII), truncation (the text grows by about
//! 30 %) and clipping (the `[` `]` markers) without a real translation.
//!
//! The transform works on the XML-escaped text exactly as lupdate writes it, so entities
//! (`&amp;`, `&lt;b&gt;` markup), `<byte value=".."/>` escapes and `%1` / `%n` / `%L1`
//! placeholders are copied through untouched and the result stays valid XML.

use anyhow::{Context, Result};

/// Fills every current message of a `.ts` file with the pseudo-translation of its source and
/// marks it finished. Numerus messages get every `<numerusform>` filled. Vanished and obsolete
/// messages are left alone. The result only depends on the sources, so re-running it is a no-op.
///
/// # Errors
///
/// Fails on a truncated `<message>`, `<source>` or `<translation>` element.
pub fn fill_ts(ts: &str) -> Result<String> {
    let mut out = String::with_capacity(ts.len() * 2);
    let mut rest = ts;
    while let Some(start) = find_element(rest, "message") {
        out.push_str(&rest[..start]);
        let block = &rest[start..];
        let end = block
            .find("</message>")
            .context("unterminated <message> element")?
            + "</message>".len();
        out.push_str(&fill_message(&block[..end])?);
        rest = &block[end..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Byte offset of the next `<name` opening tag (followed by `>`, `/` or whitespace, so
/// `<numerusform` never matches `<numerus`).
fn find_element(text: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(pos) = text[from..].find(&open) {
        let at = from + pos;
        let next = text[at + open.len()..].chars().next();
        if next.is_some_and(|c| c == '>' || c == '/' || c.is_ascii_whitespace()) {
            return Some(at);
        }
        from = at + open.len();
    }
    None
}

/// Rewrites the `<translation>` of one `<message>...</message>` block.
fn fill_message(block: &str) -> Result<String> {
    let open_end = block.find('>').context("malformed <message> tag")?;
    let numerus = block[..open_end].contains(r#"numerus="yes""#);
    let Some(source_start) = block.find("<source>") else {
        // An id-based message without source text: nothing to pseudo-translate.
        return Ok(block.to_owned());
    };
    let text_start = source_start + "<source>".len();
    let text_len = block[text_start..]
        .find("</source>")
        .context("unterminated <source> element")?;
    let pseudo = pseudo_translate(&block[text_start..text_start + text_len]);

    let Some(tr_start) = find_element(block, "translation") else {
        return Ok(block.to_owned());
    };
    let tr_tag_end = tr_start
        + block[tr_start..]
            .find('>')
            .context("malformed <translation> tag")?;
    let tag = &block[tr_start..tr_tag_end];
    if tag.contains(r#"type="vanished""#) || tag.contains(r#"type="obsolete""#) {
        return Ok(block.to_owned());
    }
    let self_closing = tag.ends_with('/');
    let finished_tag = tag
        .trim_end_matches('/')
        .replace(r#" type="unfinished""#, "")
        .trim_end()
        .to_owned();

    let (inner, after) = if self_closing {
        ("", &block[tr_tag_end + 1..])
    } else {
        let inner_start = tr_tag_end + 1;
        let inner_len = block[inner_start..]
            .find("</translation>")
            .context("unterminated <translation> element")?;
        (
            &block[inner_start..inner_start + inner_len],
            &block[inner_start + inner_len + "</translation>".len()..],
        )
    };
    let filled = if numerus {
        fill_numerus_forms(inner, &pseudo)?
    } else {
        pseudo
    };
    Ok(format!(
        "{}{finished_tag}>{filled}</translation>{after}",
        &block[..tr_start]
    ))
}

/// Replaces the content of every `<numerusform>` (keeping lupdate's layout between them). A
/// translation without any gets a single form.
fn fill_numerus_forms(inner: &str, pseudo: &str) -> Result<String> {
    let mut out = String::with_capacity(inner.len() + pseudo.len() * 3);
    let mut rest = inner;
    let mut forms = 0;
    while let Some(start) = find_element(rest, "numerusform") {
        let tag_end = start + rest[start..].find('>').context("malformed <numerusform>")?;
        let tag = &rest[start..tag_end];
        out.push_str(&rest[..start]);
        forms += 1;
        if let Some(open) = tag.strip_suffix('/') {
            out.push_str(open.trim_end());
            rest = &rest[tag_end + 1..];
        } else {
            out.push_str(tag);
            let content_len = rest[tag_end + 1..]
                .find("</numerusform>")
                .context("unterminated <numerusform>")?;
            rest = &rest[tag_end + 1 + content_len + "</numerusform>".len()..];
        }
        out.push('>');
        out.push_str(pseudo);
        out.push_str("</numerusform>");
    }
    out.push_str(rest);
    if forms == 0 {
        return Ok(format!("<numerusform>{pseudo}</numerusform>"));
    }
    Ok(out)
}

/// Pseudo-translates one XML-escaped source text: ASCII letters get accents, about 30 % padding
/// is added and the result is wrapped in brackets. Protected tokens are copied verbatim, and the
/// mnemonic letter after `&` (`&amp;File`) keeps its key. An empty source stays empty.
pub fn pseudo_translate(source: &str) -> String {
    if source.is_empty() {
        return String::new();
    }
    let mut body = String::with_capacity(source.len() * 2);
    let mut visible = 0_usize;
    let mut keep_next_letter = false;
    let mut rest = source;
    while let Some(c) = rest.chars().next() {
        if let Some((len, weight)) = protected_token(rest) {
            let token = &rest[..len];
            body.push_str(token);
            visible += weight;
            // `&&` in a mnemonic text is a literal ampersand, not a mnemonic marker.
            keep_next_letter = token == "&amp;" && !keep_next_letter;
            rest = &rest[len..];
            continue;
        }
        body.push(if keep_next_letter { c } else { accented(c) });
        keep_next_letter = false;
        visible += 1;
        rest = &rest[c.len_utf8()..];
    }
    let padding = (visible * 3).div_ceil(10).max(1);
    format!("[{body} {}]", "~".repeat(padding))
}

/// Length in bytes and visible width of a token at the start of `text` that must be copied
/// verbatim: escaped markup (`&lt;b&gt;`), an entity, a `<byte .../>` escape or a `QString::arg`
/// placeholder (`%1`..`%99`, `%L1`, `%n`, `%Ln`).
fn protected_token(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    match bytes.first()? {
        b'&' => {
            if let Some(tag) = text.strip_prefix("&lt;") {
                let starts_tag = tag
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'/' || b == b'!');
                if let Some(end) = tag.find("&gt;").filter(|_| starts_tag) {
                    return Some(("&lt;".len() + end + "&gt;".len(), 0));
                }
            }
            let end = text[1..].find(';')? + 1;
            let name = &text[1..end];
            let is_entity = (1..=10).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#');
            is_entity.then_some((end + 1, 1))
        }
        b'<' => {
            let end = text.starts_with("<byte ").then(|| text.find("/>"))??;
            Some((end + 2, 1))
        }
        b'%' => {
            let mut len = 1;
            if bytes.get(len) == Some(&b'L') {
                len += 1;
            }
            if bytes.get(len) == Some(&b'n') {
                return Some((len + 1, len + 1));
            }
            let digits = bytes[len..]
                .iter()
                .take(2)
                .take_while(|b| b.is_ascii_digit())
                .count();
            (digits > 0).then_some((len + digits, len + digits))
        }
        _ => None,
    }
}

/// Accented look-alike of an ASCII letter (Latin-1 and Latin Extended-A, which the bundled Inter
/// covers); anything else is returned unchanged.
fn accented(c: char) -> char {
    const LOWER: [char; 26] = [
        'á', 'b', 'ç', 'ď', 'é', 'ƒ', 'ĝ', 'ĥ', 'í', 'ĵ', 'ķ', 'ĺ', 'm', 'ñ', 'ö', 'þ', 'q', 'ŕ',
        'š', 'ť', 'ü', 'v', 'ŵ', 'x', 'ý', 'ž',
    ];
    const UPPER: [char; 26] = [
        'Å', 'B', 'Ç', 'Ď', 'É', 'F', 'Ĝ', 'Ĥ', 'Í', 'Ĵ', 'Ķ', 'Ĺ', 'M', 'Ñ', 'Ö', 'Þ', 'Q', 'Ŕ',
        'Š', 'Ť', 'Ü', 'V', 'Ŵ', 'X', 'Ý', 'Ž',
    ];
    let table = if c.is_ascii_lowercase() {
        &LOWER
    } else if c.is_ascii_uppercase() {
        &UPPER
    } else {
        return c;
    };
    u8::try_from(c.to_ascii_lowercase())
        .ok()
        .and_then(|byte| byte.checked_sub(b'a'))
        .and_then(|index| table.get(usize::from(index)))
        .copied()
        .unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_are_accented_padded_and_bracketed() {
        assert_eq!(pseudo_translate("Save"), "[Šávé ~~]");
        assert_eq!(pseudo_translate("OK"), "[ÖĶ ~]");
        assert_eq!(pseudo_translate(""), "");
        // Non-letters are kept; the padding is about 30 % of the visible length.
        assert_eq!(pseudo_translate("1234567890"), "[1234567890 ~~~]");
    }

    #[test]
    fn placeholders_are_preserved() {
        assert_eq!(pseudo_translate("%1 of %2"), "[%1 öƒ %2 ~~~]");
        let numerus = pseudo_translate("%n host(s), %Ln file(s)");
        assert!(numerus.contains("%n ĥöšť(š)"), "{numerus}");
        assert!(numerus.contains("%Ln ƒíĺé(š)"), "{numerus}");
        assert!(pseudo_translate("%L1 MB").starts_with("[%L1 MB"));
        assert!(pseudo_translate("Tab %12").contains("%12"));
        // A lone percent sign is plain text.
        assert_eq!(pseudo_translate("50%"), "[50% ~]");
    }

    #[test]
    fn entities_markup_and_byte_escapes_are_preserved() {
        assert_eq!(
            pseudo_translate("Fish &amp; chips &lt; 3"),
            "[Fíšĥ &amp; çĥíþš &lt; 3 ~~~~~]"
        );
        assert_eq!(
            pseudo_translate("&lt;b&gt;Bold&lt;/b&gt; &quot;x&quot; &apos;y&apos; &#x2014;"),
            "[&lt;b&gt;Böĺď&lt;/b&gt; &quot;x&quot; &apos;ý&apos; &#x2014; ~~~~~]"
        );
        assert_eq!(
            pseudo_translate(r#"a<byte value="x9"/>b"#),
            r#"[á<byte value="x9"/>b ~]"#
        );
    }

    #[test]
    fn mnemonic_letters_keep_their_key() {
        assert_eq!(pseudo_translate("&amp;Open"), "[&amp;Oþéñ ~~]");
        // `&&` is a literal ampersand, so the next letter is translated.
        assert_eq!(pseudo_translate("&amp;&amp;a"), "[&amp;&amp;á ~]");
    }

    const TS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="en" sourcelanguage="en">
<context>
    <name>OsDialog</name>
    <message>
        <source>Close</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <source>Open</source>
        <comment>verb</comment>
        <extracomment>Opens the &lt;b&gt;file&lt;/b&gt;</extracomment>
        <translation>stale</translation>
    </message>
    <message numerus="yes">
        <source>%n host(s)</source>
        <translation type="unfinished">
            <numerusform></numerusform>
            <numerusform/>
        </translation>
    </message>
    <message>
        <source>Gone</source>
        <translation type="vanished">Gone</translation>
    </message>
    <message>
        <source>Empty</source>
        <translation type="unfinished"/>
    </message>
</context>
</TS>
"#;

    const FILLED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="en" sourcelanguage="en">
<context>
    <name>OsDialog</name>
    <message>
        <source>Close</source>
        <translation>[Çĺöšé ~~]</translation>
    </message>
    <message>
        <source>Open</source>
        <comment>verb</comment>
        <extracomment>Opens the &lt;b&gt;file&lt;/b&gt;</extracomment>
        <translation>[Öþéñ ~~]</translation>
    </message>
    <message numerus="yes">
        <source>%n host(s)</source>
        <translation>
            <numerusform>[%n ĥöšť(š) ~~~]</numerusform>
            <numerusform>[%n ĥöšť(š) ~~~]</numerusform>
        </translation>
    </message>
    <message>
        <source>Gone</source>
        <translation type="vanished">Gone</translation>
    </message>
    <message>
        <source>Empty</source>
        <translation>[Émþťý ~~]</translation>
    </message>
</context>
</TS>
"#;

    #[test]
    fn ts_messages_are_filled_and_finished() {
        let filled = fill_ts(TS).unwrap();
        assert_eq!(filled, FILLED);
        assert!(!filled.contains(r#"type="unfinished""#));
    }

    #[test]
    fn filling_is_idempotent() {
        let once = fill_ts(TS).unwrap();
        assert_eq!(fill_ts(&once).unwrap(), once);
    }

    #[test]
    fn truncated_files_are_rejected() {
        assert!(fill_ts("<message><source>x</source><translation>").is_err());
        assert!(fill_ts("<message><source>x").is_err());
        assert!(fill_ts("<TS><message>").is_err());
        assert!(fill_ts("<message><source>x</source><translation>y</message>").is_err());
        assert!(fill_ts("<message numerus=\"yes\"><source>x</source><translation><numerusform>y</translation></message>").is_err());
    }
}
