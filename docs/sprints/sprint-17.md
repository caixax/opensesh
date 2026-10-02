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
- [ ] `fuzz/` with targets for quick connect, the importers, the theme parsers, the paste analyzer, the monitor's parsers and the sync merge
- [ ] Seed corpora from the fixtures; a CI workflow (an hour per target, by hand and weekly)
- [ ] Findings fixed with regression tests (importers read only regular files of a bounded size)

### Performance
- [ ] §9 budgets measured again on the current app; `docs/perf.md`
- [ ] Views loaded on first use; a leak check (tabs and dialogs opened and closed)

### Accessibility
- [ ] High-contrast theme (setting and system preference), with its contrast checked automatically
- [ ] Contrast check over every accent preset
- [ ] Accessible names everywhere (lint rule for icon-only buttons); focus order

### Security, i18n, errors, onboarding
- [ ] Security review written down; what it finds fixed
- [ ] i18n: no strings without `qsTr()`, pseudo-locale complete
- [ ] Error messages with unfolding technical details
- [ ] First-run onboarding: import, new host or local terminal, in at most three steps

### Quality
- [ ] **Done when:** the fuzzers run for an hour without crashes, the §9 budgets are met, and the manual matrix is complete
- [ ] Smoke test and screenshots: high contrast, onboarding, error details

### Close
- [ ] fmt, clippy `-D warnings`, tests, lint-qml, i18n, deny, audit
- [ ] Build and smoke tests on Windows and Linux (CI); GitHub Actions green
- [ ] ADRs, docs, CHANGELOG, report, commits pushed to GitHub

## Report

(Filled in at the end of the sprint.)
