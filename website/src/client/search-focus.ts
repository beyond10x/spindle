// The search plugin loads its index on the field's first focus and focuses the field when the index
// has loaded, even when focus has left it since (`focusAfterIndexLoaded` in the plugin's SearchBar
// is set on focus and never cleared on blur). A keyboard reader who tabs past the field before the
// index arrives is pulled back to it (UXF-021). This drops a focus call on the field made outside a
// user's input event (the index load finishes in a worker's message event) while another element
// has focus. With focus on the field or on nothing (the plugin wraps the field when the index
// loads, which drops its focus, and focuses it again), and from a key, pointer or touch event such
// as the plugin's keyboard shortcut, the call goes through.

const guarded = new WeakSet<HTMLInputElement>();

function guard(): void {
  const field = document.querySelector<HTMLInputElement>('input.navbar__search-input');
  if (!field || guarded.has(field)) return;
  guarded.add(field);
  const focus = field.focus.bind(field);
  field.focus = (options?: FocusOptions) => {
    const active = document.activeElement;
    const elsewhere = active !== null && active !== document.body && active !== field;
    if (elsewhere && !(window.event instanceof UIEvent)) return;
    focus(options);
  };
}

export function onRouteDidUpdate(): void {
  guard();
}
