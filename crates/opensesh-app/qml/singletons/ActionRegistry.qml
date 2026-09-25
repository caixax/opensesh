pragma Singleton

// Central list of user actions (id, title, shortcut, handler). The shell binds one Shortcut per
// action and the command palette lists them, so every command is reachable from the keyboard.
import QtQuick

QtObject {
    id: registry

    // OsAction objects, in registration order.
    property var actions: []

    // Emitted after any action runs (used for "recently used" in the palette).
    signal ran(string actionId)

    function register(action) {
        if (!action || !action.actionId) {
            console.warn("ActionRegistry: refusing an action without actionId");
            return;
        }
        if (find(action.actionId)) {
            console.warn("ActionRegistry: duplicate action id", action.actionId);
            return;
        }
        actions = actions.concat([action]);
    }

    function unregister(action) {
        actions = actions.filter(existing => existing !== action);
    }

    function find(actionId) {
        for (const action of actions) {
            if (action.actionId === actionId)
                return action;
        }
        return null;
    }

    function trigger(actionId) {
        const action = find(actionId);
        if (!action) {
            console.warn("ActionRegistry: unknown action", actionId);
            return false;
        }
        if (!action.enabled)
            return false;
        action.trigger();
        ran(actionId);
        return true;
    }

    // Actions whose shortcut is used by more than one action (conflict detection, §6.4).
    function conflicts() {
        const seen = {};
        const out = [];
        for (const action of actions) {
            if (!action.shortcut)
                continue;
            const key = action.shortcut.toLowerCase();
            if (seen[key])
                out.push([seen[key].actionId, action.actionId]);
            else
                seen[key] = action;
        }
        return out;
    }

    // Fuzzy match score of `query` in `text` (higher is better, -1 means no match): every query
    // character must appear in order; consecutive and word-start matches score more.
    function fuzzyScore(query, text) {
        const q = query.toLowerCase();
        const t = text.toLowerCase();
        if (q.length === 0)
            return 0;
        let score = 0;
        let ti = 0;
        let streak = 0;
        for (let qi = 0; qi < q.length; ++qi) {
            const found = t.indexOf(q[qi], ti);
            if (found < 0)
                return -1;
            const wordStart = found === 0 || " .-_/".indexOf(t[found - 1]) >= 0;
            streak = found === ti ? streak + 1 : 0;
            score += 1 + streak * 2 + (wordStart ? 3 : 0) - Math.min(found - ti, 3) * 0.5;
            ti = found + 1;
        }
        return score - t.length * 0.01;
    }

    // Palette entries matching `query`, best first: [{ action, score }].
    function search(query) {
        const results = [];
        for (const action of actions) {
            if (!action.showInPalette)
                continue;
            const haystack = action.category + " " + action.text + " " + action.actionId;
            const score = Math.max(fuzzyScore(query, action.text) + 1, fuzzyScore(query, haystack));
            if (score >= 0)
                results.push({ action: action, score: score });
        }
        results.sort((a, b) => b.score - a.score || a.action.text.localeCompare(b.action.text));
        return results;
    }
}
