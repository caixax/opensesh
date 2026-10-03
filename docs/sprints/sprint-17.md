# Sprint 17: polish, performance, accessibility and security

**Goal:** make it feel like 1.0.

**Started:** 2026-10-03

## Scope notes (decided at the start)

- **Owner overrides still apply:** English only. The repository is public on GitHub, and the internal planning files stay out of it. The owner asked on 2026-10-02 to keep going from sprint to sprint without waiting.
- **Fuzzing** (`cargo-fuzz`, libFuzzer; a `fuzz/` workspace of its own so the app still builds on stable): quick connect's parser, every importer (`~/.ssh/config`, MobaXterm, PuTTY, Remmina, CSV, bundles), the terminal theme parsers, the paste analyzer, the remote monitor's parsers, and the sync merge with the conflict splitter. Seed corpora from the existing fixtures. A CI workflow runs each target for an hour (by hand and weekly); short runs locally in WSL. What the fuzzers find is fixed with a regression test.
- **Performance:** the PLAN §9 budgets measured again on the current app (release build, Windows; Linux in WSLg as before): cold start, memory at rest with one terminal, the hosts view with 1000 hosts; views that aren't shown at start are loaded on first use; a leak check that opens and closes many tabs and dialogs and compares memory. Results in `docs/perf.md`.
- **Accessibility:**
  - **A high-contrast theme:** a setting (and the system's high-contrast preference, `QAccessibilityHints::contrastPreference` from Qt 6.10) that makes text, borders and the focus ring stronger: text at 7:1 (AAA), controls' outlines and indicators at 4.5:1.
  - **The automated contrast check** extended to the high-contrast tokens and to every accent preset.
  - **A pass over the components and views:** every interactive control has an accessible name (a `lint-qml` rule for icon-only buttons), the focus order, focus rings.
- **Security review** against the threat model and the ADRs, written down in `docs/security-review.md` with what was checked, what was found and what was fixed.
- **i18n:** no user-visible string without `qsTr()` (lint), the pseudo-translation complete and checked in CI; the Spanish translation stays postponed (owner's choice), so "no untranslated strings" is checked with the pseudo-locale.
- **Error messages:** human words first, with a "Technical details" section that unfolds (a component), on the error surfaces that still show raw errors.
- **Onboarding:** the first start offers, in at most three steps: import hosts, create a host, or open a local terminal.

## Checklist

### Fuzzing
- [x] `fuzz/` with targets for quick connect, the importers, the theme parsers, the paste analyzer, the monitor's parsers and the sync merge
- [x] Seed corpora from the fixtures; a CI workflow (an hour per target, by hand and weekly)
- [x] Findings fixed with regression tests (importers read only regular files of a bounded size)

### Performance
- [x] §9 budgets measured again on the current app; `docs/perf.md`
- [x] Views loaded on first use; a leak check (tabs and dialogs opened and closed)

### Accessibility
- [x] High-contrast theme (setting and system preference), with its contrast checked automatically
- [x] Contrast check over every accent preset
- [x] Accessible names everywhere (lint rule for icon-only buttons); focus order

### Security, i18n, errors, onboarding
- [x] Security review written down; what it finds fixed
- [x] i18n: no strings without `qsTr()`, pseudo-locale complete
- [x] Error messages with unfolding technical details
- [x] First-run onboarding: import, new host or local terminal, in at most three steps

### Quality
- [ ] **Done when:** the fuzzers run for an hour without crashes, the §9 budgets are met, and the manual matrix is complete
- [x] Smoke test and screenshots: high contrast, onboarding, error details

### Close
- [x] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [x] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [x] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

**Closed:** 2026-10-03

### What was done

- **Fuzzing** ([ADR 0038](../adr/0038-polish-before-1-0.md)): `fuzz/` (cargo-fuzz, libFuzzer, a nightly workspace of its own) with eleven targets: quick connect, the paste analyzer, the six terminal theme formats, the remote monitor's parsers, `~/.ssh/config`, MobaXterm, PuTTY, Remmina, CSV, bundles, and the sync merge with Git's conflict splitter. Seeds from the fixtures (`fuzz/seed.sh`), `fuzz/run-all.sh` for a local pass, and a `Fuzz` workflow that runs each target for an hour by hand and every Sunday.
- **What fuzzing found, fixed with regression tests:**
  - the remote monitor's macOS swap figure was split one byte before its end, which panicked when the server's text ended in a character of several bytes;
  - the remote monitor added up a server's counters (CPU ticks, memory pages, network bytes) with plain additions, which overflowed (a panic in debug builds) when the server printed huge numbers: every sum saturates now;
  - `Include /*/*` in `~/.ssh/config` read the whole disk (now at most 256 files and 8 MiB);
  - a `nan` in a settings file was a conflict on every merge (`NaN != NaN`).
- **Security review** (`docs/security-review.md`), against the threat model and the ADRs: the defenses traced to code and tests; two more findings fixed (importers read only regular files of a bounded size, so `Include /dev/zero` or a FIFO can't hang an import; the single-instance socket no longer moves with the settings folder); the updater's authenticity and two dependency advisories written down as open or accepted; the threat model points to it.
- **Performance** (`docs/perf.md`): start-up and memory measured again. Seventeen dialogs, the pane's context menu and the tab's paste review are now created on first use (`LazyPopup`): start-up 889 → 720 ms, the idle window 168 → 137 MB on the measuring machine. A leak check in the smoke test.
- **Accessibility:** a high-contrast theme (a Contrast setting following Qt 6.10's system preference: text at 7:1, outlines, focus ring and status colors at 4.5:1, a 3 px focus ring), checked with both schemes and every accent preset; `lint-qml` asks every icon-only button for a name (all 236 had one).
- **Error messages:** toasts say what happened in plain words; the technical text (the library's or system's error) is behind "Details", in a dialog where it can be selected and copied, also from the notifications panel. The fifteen toasts that pasted raw errors were changed.
- **Onboarding:** the empty Hosts view welcomes a first start with import hosts, a new host, a local terminal and quick connect, one click each.
- **i18n:** no user-visible string without `qsTr()` (lint), and the pseudo-locale has every string (2928, none unfinished, checked in CI); the Spanish translation stays postponed (owner's choice).
- **Documentation:** ADR 0038, the security review, `docs/perf.md`, the component contract (`highContrast`, `LazyPopup`, toast details, the lint rule), the threat model, CHANGELOG and the manual matrix.

### "Done when" (PLAN)

- **The fuzzers run for an hour without crashes:** **met.** Three runs of an hour per target on GitHub's runners: the first found three crashes and the second a fourth, all fixed with regression tests; the third, on the fixed code, went through all eleven targets without a crash.
- **The §9 budgets are met:** start-up **met** (720 ms, under 1 s; the 500 ms target missed). Memory with one terminal **met with the software renderer** (137 MB) **but not with D3D11 on the measuring machine's NVIDIA card** (166.7 MB; the driver takes about 30 MB of private memory, Sprint 2). The other budgets weren't measured again: the engine, its renderer and the hosts search haven't changed.
- **The manual matrix is complete:** not by me: the matrix lists what only the owner can check (desktops, screen readers, real files, two computers syncing), still marked ⏳ for Sprints 13 to 17.

### How it was verified

- **Windows (Qt 6.10.3), on the final code:** fmt, clippy `-D warnings`, lint-qml, i18n, `cargo deny`, `cargo audit`; every test (738 in the app's workspace); the smoke tests offscreen with the software renderer, native and the gallery; the screenshots (240, 16 new).
- **WSL (Debian):** every fuzz target for a minute after each fix.
- **GitHub Actions:** green on the sprint's last code commit, all ten jobs: format, lints, licenses and advisories; Ubuntu 24.04 with aqt's Qt; Windows; the Arch, Fedora and Debian 13 containers with their own Qt; SSH against OpenSSH and Dropbear; S3 against RustFS; RDP against xrdp; VNC against TigerVNC, x11vnc and wayvnc. Debian 13's smoke test had failed once on the high-contrast binding (below).

### Problems found and fixed

- **A flaky tunnel test** in the Arch container: a stopped forward's port was checked after a fixed 200 ms; it is now awaited (up to 5 s).
- **A first lint rule that clippy rejected** (an `if` that could be collapsed) was fixed before it was pushed.
- **SFTP hosts** (and `sftp://` in quick connect) still answered that SFTP "arrives in Sprint 8", a leftover from before Sprint 8: they open the files view now, checked by a smoke test step, and the rest of the "arrives in Sprint N" code, unreachable since Sprint 14, is gone.
- **The high-contrast binding on Qt 6.8:** `Application.styleHints.accessibility` is new in Qt 6.10, and Debian 13's smoke test failed on the warning; the system's preference is read only where it exists.

### Deviations

- **Memory with one terminal on Windows with D3D11** is above 150 MB on the measuring machine (see "Done when"); a terminal tab costs 22 MB more than in Sprint 2 even with the software renderer, and is the next place to look.
- **The manual matrix** needs the owner.
