// The screen in the URL fragment, so the `?t=` token in the query stays where it is.

export type Route =
  | { screen: 'topology' }
  | { screen: 'instance'; instance: string }
  | { screen: 'view'; component: string; view: string };

export function route(hash: string): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent);
  if (parts[0] === 'instance' && parts.length === 2) return { screen: 'instance', instance: parts[1]! };
  if (parts[0] === 'view' && parts.length === 3) {
    return { screen: 'view', component: parts[1]!, view: parts[2]! };
  }
  return { screen: 'topology' };
}
