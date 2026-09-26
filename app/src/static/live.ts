// Часть сайта `static-live.js`: живые блоки заметки (рантайм Svelte,
// разборщик формул, граф с физикой). Грузится, только если на странице
// есть живой блок (`main.ts`).

import { mountLive } from "../lib/live";
import { register } from "./parts";

register("live", { mountLive });
