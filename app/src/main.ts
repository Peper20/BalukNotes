import { mount } from "svelte";
import { pickVault } from "./lib/boot";
import "./app.css";

// Хранилище — раньше состояния клиента (вкладки и места чтения у каждого
// свои), поэтому App и модули состояния загружаются после. Какое — не
// ясно (первый запуск) — экран выбора хранилища.
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
