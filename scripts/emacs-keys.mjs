// The two presets of Emacs's keys in VS Code and Sublime Text. `emacs_full`
// has every key Emacs has. `emacs`, the default, leaves out those an editor
// uses for its text, so that in Org files and capture buffers Shift,
// Alt+Shift and Ctrl+Shift with the arrows select, and Ctrl+A, Ctrl+E,
// Ctrl+K, Ctrl+O, Ctrl+X, Ctrl+Y, Ctrl+Enter and Ctrl+Shift+Enter do what
// they do in any file; there Org's `C-c` keys start only where nothing is
// selected, so that Ctrl+C copies a selection. The agenda, the date prompt
// and column view keep every key in both: they hold no text to edit.

const TEXT = ['org', 'capture'];

/** Whether `emacs` leaves out the Emacs key `key` (`S-<left>`, `C-x n s`)
 * where `scope` is (`org`, `capture`, `agenda` …). */
export function leftOut(key, scope) {
  const first = key.split(' ')[0];
  return (
    TEXT.includes(scope) &&
    (/^(C-|M-)?S-<(up|down|left|right)>$/.test(first) || /^C-[aekoxy]$/.test(first) || /^C-(S-)?RET$/.test(first))
  );
}

/** Whether, in `emacs`, the key starts only where nothing is selected:
 * Org's `C-c` keys in text, as Ctrl+C copies a selection. */
export function needsNoSelection(key, scope) {
  return TEXT.includes(scope) && key.split(' ')[0] === 'C-c';
}
