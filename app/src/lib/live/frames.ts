// The frames are already in the markup; a slider and "▶" are added,
// `data-live` turns on showing the current frame instead of the default one.

import { mount, unmount } from "svelte";
import Frames from "../../components/Frames.svelte";
import { parseFrames } from "../frames";
import type { LiveBlock } from "./block";
import { SELECTORS } from "./selectors";

export const frames: LiveBlock = {
  name: "frames",
  selector: SELECTORS.frames,
  mount(el) {
    const spec = parseFrames(el.dataset.kFrames);
    const items = [...el.querySelectorAll<HTMLElement>(":scope > .k-frames-stack > .k-frames-item")];
    if (!spec || items.length !== spec.count) return null;
    const m = mount(Frames, { target: el, props: { root: el, items, spec } });
    return () => void unmount(m);
  },
};
