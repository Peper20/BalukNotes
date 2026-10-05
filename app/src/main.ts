import { mount } from "svelte";
import { pickVault } from "./lib/boot";
import "./app.css";

// The vault comes before the client state (tabs and reading places are per
// vault), so App and the state modules load after it. If it is unclear
// which one (the first start) - the vault picker screen.
try {
  const choose = await pickVault();
  if (choose) {
    const { default: VaultPicker } = await import("./components/VaultPicker.svelte");
    mount(VaultPicker, { target: document.body, props: { list: choose } });
  } else {
    const { default: App } = await import("./App.svelte");
    mount(App, { target: document.body });
  }
} catch (e) {
  const p = Object.assign(document.createElement("p"), { className: "fatal", textContent: `Не удалось запустить клиент: ${(e as Error).message}` });
  document.body.replaceChildren(p);
}
