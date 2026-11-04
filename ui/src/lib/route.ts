// The screen in the URL fragment, so the `?t=` token in the query stays where it is.

export type Route =
  | { screen: 'topology' }
  | { screen: 'instance'; instance: string }
  | { screen: 'view'; component: string; view: string };

// A fragment is whatever the address bar holds; one that does not decode names no screen.
export function route(hash: string): Route {
  let parts: string[];
  try {
    parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent);
  } catch {
    return { screen: 'topology' };
  }
  if (parts[0] === 'instance' && parts.length === 2) return { screen: 'instance', instance: parts[1]! };
  if (parts[0] === 'view' && parts.length === 3) {
    return { screen: 'view', component: parts[1]!, view: parts[2]! };
  }
  return { screen: 'topology' };
}
