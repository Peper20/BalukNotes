// The vault graph (`#vault-graph`): the same coordinates as the Typst
// picture, plus hover and going to a note. CSS hides the picture (`[data-live]`).

import { mount, unmount } from "svelte";
import Graph from "../../components/Graph.svelte";
import type { GraphLayout } from "../api";
import type { LiveBlock } from "./block";
import { SELECTORS } from "./selectors";

export const graph: LiveBlock = {
  name: "vault graph",
  selector: SELECTORS.graph,
  mount(el, { open }) {
    const layout = JSON.parse(el.dataset.kGraph ?? "") as GraphLayout;
    if (!Array.isArray(layout.nodes) || !layout.nodes.length) return null;
    const target = el.appendChild(Object.assign(document.createElement("div"), { className: "k-graph-live" }));
    const m = mount(Graph, { target, props: { layout, onopen: open } });
    return () => void unmount(m);
  },
};
