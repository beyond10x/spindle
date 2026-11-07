// Registers the Service Worker built beside the page and resolves once it controls the page, so
// that every fetch the console sends from then on is answered in the browser.

export async function register(): Promise<void> {
  if (!('serviceWorker' in navigator)) {
    throw new Error('this browser has no Service Worker');
  }
  const workers = navigator.serviceWorker;
  await workers.register(new URL('sw.js', document.baseURI), { type: 'module', scope: './' });
  await workers.ready;
  if (workers.controller === null) {
    await new Promise<void>((resolve) => {
      workers.addEventListener('controllerchange', () => resolve(), { once: true });
    });
  }
}
