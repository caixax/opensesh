# ADR 0041: The first UI translations

- **Status:** accepted
- **Date:** 2026-11-20
- **Issue:** [#35](https://github.com/caixax/opensesh/issues/35)

## Context

ADR 0009 built the translation pipeline and kept the interface in English only, with a pseudo-locale to exercise it. Issue #35 asked for a Simplified Chinese translation and offered to write it. On 2026-11-20 the owner asked for Chinese and for more languages.

The interface has 2934 messages, 2349 of them distinct. About 320 belong only to the component gallery (`--gallery`), a developer tool.

## Options

1. Simplified Chinese only, as #35 asked.
2. A handful of widely used languages at once, from the same pass over the strings.
3. Wait for volunteer translators, one language at a time.

## Decision

Option 2: German, Spanish, French, Japanese, Brazilian Portuguese and Simplified Chinese (`de`, `es`, `fr`, `ja`, `pt_BR`, `zh_CN` in `assets/i18n/languages.toml`).

- Every message of the app is translated, except those of the component gallery, which stay in English.
- Product, protocol and file names (OpenSSH, `~/.ssh/config`, `hosts.toml`) and option values that the user types (`publickey`, `socks5://host:port`) are not translated. Shortcuts in menus keep the key names Qt shows elsewhere in the app (Ctrl+C, F5); sentences may use the keyboard's local names.
- With "System default", a system language that carries a script, such as `zh-Hans-CN`, also tries its `QLocale` name (`zh_CN`), so a Chinese Windows finds `opensesh_zh_CN.qm`.
- The language list names each language in itself, with its territory when the code has one, as in "Português (Brasil)".
- Labels that a screenshot run showed cut off were shortened in the languages concerned (the host filters, "Data and sync").
- The bundled Inter and JetBrains Mono have no Chinese or Japanese glyphs. Those come from the system's fallback fonts, as in the terminal (ADR 0013): Windows ships them, and Linux needs a CJK font such as Noto Sans CJK.

The translations were made without native speakers' review. They are a starting point: corrections through the `.ts` files are welcome, and the release notes say so.

## Consequences

- Each new or changed message shows in English in every language until someone translates it; `cargo xtask i18n` keeps the finished translations and marks the new ones unfinished.
- Dialogs must size themselves from their content's implicit width, not from their buttons: a short translation made the tunnel import dialog narrower than its content. `--screenshots` catches this per language when run with `language` set in a portable `config.toml`. The smoke test checks English text, so it always runs in English.
- Adding a language is still what ADR 0009 says: a code in `languages.toml`, the translated `.ts`, and `cargo xtask i18n`.
