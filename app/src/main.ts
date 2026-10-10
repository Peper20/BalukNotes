import { mount, unmount } from "svelte";
import { api, onSignInRequired, signInRequired } from "./lib/api";
import { pickVault } from "./lib/boot";
import { session } from "./lib/state/session.svelte";
import "./app.css";

// What is on the screen now: the app, the vault picker or the sign-in screen.
let shown: Record<string, unknown> | undefined;

// A request answered 401 (a server with sign-in, no session - at the start or
// when the session ended): the sign-in screen instead of whatever is shown.
async function showSignIn() {
  const { default: SignIn } = await import("./components/SignIn.svelte");
  if (shown) unmount(shown);
  document.body.replaceChildren();
  shown = mount(SignIn, { target: document.body });
}
onSignInRequired(() => void showSignIn());

// The vault comes before the client state (tabs and reading places are per
// vault), so App and the state modules load after it. If it is unclear
// which one (the first start) - the vault picker screen.
try {
  // Whether this server wants a sign-in (the login is shown in the menu); a
  // 401 here is the sign-in screen, through `onSignInRequired`.
  session.login = await api.session();
  const choose = await pickVault();
  if (choose) {
    const { default: VaultPicker } = await import("./components/VaultPicker.svelte");
    if (!signInRequired()) shown = mount(VaultPicker, { target: document.body, props: { list: choose } });
  } else {
    const { default: App } = await import("./App.svelte");
    if (!signInRequired()) shown = mount(App, { target: document.body });
  }
} catch (e) {
  if (!signInRequired()) {
    const p = Object.assign(document.createElement("p"), { className: "fatal", textContent: `Не удалось запустить клиент: ${(e as Error).message}` });
    document.body.replaceChildren(p);
  }
}
