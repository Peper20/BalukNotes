<!--
  The sign-in screen: the server (`notes serve --auth`) wants a session and
  there is none (the first visit, the session ended, sign-out). Instead of the
  app. After a successful sign-in the page reloads to the same address. The
  settings are behind the session, so the theme here is the one the browser
  remembered (`public/assets/theme.js`) or, in a browser that remembered
  none, the system's (the theme list is open).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, signInMessage } from "../lib/api";
  import { resolveTheme } from "../lib/appearance";

  let login = $state("");
  let password = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  onMount(() => {
    const root = document.documentElement;
    root.dataset.state = "ready";
    if (root.dataset.theme) return;
    // A first visit: no remembered theme, so no colors; the system's light or dark one.
    const dark = matchMedia("(prefers-color-scheme: dark)");
    void api.themes().then((themes) => {
      const apply = () => (root.dataset.theme = resolveTheme("auto", themes, dark.matches));
      apply();
      dark.addEventListener("change", apply);
    }, () => {});
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || !login.trim() || !password) return;
    busy = true;
    error = null;
    try {
      await api.login(login.trim(), password);
      // The cookie is set: the same address now opens the app. `busy` stays on until the reload.
      location.reload();
    } catch (err) {
      error = signInMessage(err);
      busy = false;
    }
  }
</script>

<main class="sign-in" id="sign-in">
  <h1>Вход</h1>
  <p class="sign-in-lead">Чтобы открыть заметки, войдите.</p>
  <form class="sign-in-form" onsubmit={submit}>
    <label for="sign-in-login">Логин</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      id="sign-in-login"
      type="text"
      bind:value={login}
      maxlength="64"
      autocomplete="username"
      autocapitalize="off"
      spellcheck="false"
      autofocus
      required
    />
    <label for="sign-in-password">Пароль</label>
    <input id="sign-in-password" type="password" bind:value={password} autocomplete="current-password" required />
    <button type="submit" disabled={busy || !login.trim() || !password}>Войти</button>
    {#if error}<p class="dialog-error" role="alert">{error}</p>{/if}
  </form>
</main>
