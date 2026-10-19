/**
 * "Add claim" from the tree: the tree selects the node, then asks the node
 * page's claims section to open its editor with a fresh row. The section may
 * mount a moment after the selection, so the request is a window event the
 * section listens for, sent once the page has had a frame to render.
 */

const EVENT = "scryer:add-claim";

export function requestAddClaim(nodeId: string) {
  window.setTimeout(() => window.dispatchEvent(new CustomEvent<string>(EVENT, { detail: nodeId })), 200);
}

/** Run `open` when a claim is requested for `hostId`. Returns the unsubscribe. */
export function onAddClaimRequest(hostId: string, open: () => void): () => void {
  const on = (e: Event) => {
    if ((e as CustomEvent<string>).detail === hostId) open();
  };
  window.addEventListener(EVENT, on);
  return () => window.removeEventListener(EVENT, on);
}
