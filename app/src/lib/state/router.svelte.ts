// Routing: page address -> what to show (a note, home, the graph, tags).
// Navigation inside the client, tabs, "back" to the reading place.

import { hashAnchor, homeHref, noteHref, parseRoute, type Route } from "../ids";
import { inVault } from "../vault";
import { placeFromHistory } from "../places";
import { notes } from "./notes.svelte";
import { places } from "./places.svelte";
import { reader } from "./reader.svelte";
import { tabs } from "./tabs.svelte";

class Router {
  route = $state.raw<Route>({ kind: "home" });
  /** The address anchor; `anchorSeq` grows on an anchor navigation within the same note. */
  anchor = $state<string | null>(null);
  anchorSeq = $state(0);

  currentId = $derived(this.route.kind === "note" ? this.route.id : null);
  /** The open folder page (path). */
  currentFolder = $derived(this.route.kind === "folder" ? this.route.path : null);
  currentNote = $derived(notes.byId(this.currentId));

  /**
   * Navigates inside the client (as by a link); `newTab` - in a new tab. An
   * address without a vault (`/n/...` from the note HTML) goes to the shown vault.
   */
  go(url: string | URL, { replace = false, newTab = false } = {}): void {
    url = inVault(String(url));
    reader.remember();
    if (newTab) tabs.openAfter(String(url));
    if (replace) history.replaceState(null, "", url);
    else history.pushState(null, "", url);
    this.sync();
  }

  /** A note; `background` - in a new background tab (Ctrl+click). */
  open(id: string, anchor?: string | null, { background = false } = {}): void {
    if (background) this.behind(noteHref(id, anchor));
    else this.go(noteHref(id, anchor));
  }

  /** Opens in a new background tab: the page and the active tab do not change. */
  behind(url: string | URL): void {
    tabs.openBehind(inVault(String(url)));
  }

  switchTab(i: number): void {
    const tab = tabs.list[i];
    if (!tab || i === tabs.active) return;
    reader.remember();
    tabs.active = i;
    history.pushState(null, "", tab.url);
    this.sync();
  }

  closeTab(i: number): void {
    if (i < 0 || i >= tabs.list.length) return;
    if (tabs.list.length === 1) {
      this.go(homeHref());
      return;
    }
    if (tabs.close(i)?.wasActive) {
      history.replaceState(null, "", tabs.list[tabs.active]!.url);
      this.sync();
    }
    tabs.save();
  }

  /** The route by the page address: the same note - only the anchor. */
  sync({ pop = false } = {}): void {
    const route = parseRoute(location.pathname, location.search);
    this.anchor = hashAnchor(location.hash);
    tabs.setUrl(location.pathname + location.search + location.hash);
    // "Back" goes where we were in this history entry; otherwise where this
    // note was read last time (if the address has no anchor).
    const fromHistory = pop ? placeFromHistory(history.state) : null;
    if (route.kind === "note" && route.id === this.currentId && !reader.pending) {
      reader.restore = this.anchor ? null : fromHistory;
      this.anchorSeq++;
      return;
    }
    this.route = route;
    if (route.kind === "note") reader.show(route.id, fromHistory ?? (this.anchor ? null : places.get(route.id)));
    else reader.clear();
  }
}

export const router = new Router();
