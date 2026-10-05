// An interactive figure goes next to the Typst frame, CSS hides the frame
// (`[data-live]`). A formula did not parse - the frame stays.

import { mount, unmount } from "svelte";
import Plot from "../../components/Plot.svelte";
import { formulasOk, readSpec } from "../plot/spec";
import type { LiveBlock } from "./block";
import { SELECTORS } from "./selectors";

export const plot: LiveBlock = {
  name: "interactive figure",
  selector: SELECTORS.plot,
  mount(el) {
    const spec = readSpec(el);
    if (!spec || !formulasOk(spec)) return null;
    const m = mount(Plot, { target: el, props: { spec } });
    return () => void unmount(m);
  },
};
