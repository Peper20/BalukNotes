// Граф хранилища (`#vault-graph`): те же координаты, что у картинки Typst,
// плюс наведение и переход к заметке. Картинку прячет CSS (`[data-live]`).

import { mount, unmount } from "svelte";
import Graph from "../../components/Graph.svelte";
import type { GraphLayout } from "../api";
import type { LiveBlock } from "./block";
import { SELECTORS } from "./selectors";

export const graph: LiveBlock = {
  name: "граф хранилища",
  selector: SELECTORS.graph,
  mount(el, { open }) {
    const layout = JSON.parse(el.dataset.kGraph ?? "") as GraphLayout;
    if (!Array.isArray(layout.nodes) || !layout.nodes.length) return null;
    const target = el.appendChild(Object.assign(document.createElement("div"), { className: "k-graph-live" }));
    const m = mount(Graph, { target, props: { layout, onopen: open } });
    return () => void unmount(m);
  },
};
