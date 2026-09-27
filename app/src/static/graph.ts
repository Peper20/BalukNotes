// Часть сайта `static-graph.js`: страница графа (`graph.html`) — граф
// хранилища из данных сборки (`assets/data/graph.js`).

import { mount } from "svelte";
import SiteGraph from "../components/SiteGraph.svelte";
import { register } from "./parts";

register("graph", {
  mount(target, layout, open) {
    target.replaceChildren();
    mount(SiteGraph, { target, props: { layout, onopen: open } });
  },
});
