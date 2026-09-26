// Интерактивный рисунок встаёт рядом с кадром Typst, кадр прячет CSS
// (`[data-live]`). Формула не разобралась — остаётся кадр.

import { mount, unmount } from "svelte";
import Plot from "../../components/Plot.svelte";
import { formulasOk, readSpec } from "../plot/spec";
import type { LiveBlock } from "./block";
import { SELECTORS } from "./selectors";

export const plot: LiveBlock = {
  name: "интерактивный рисунок",
  selector: SELECTORS.plot,
  mount(el) {
    const spec = readSpec(el);
    if (!spec || !formulasOk(spec)) return null;
    const m = mount(Plot, { target: el, props: { spec } });
    return () => void unmount(m);
  },
};
