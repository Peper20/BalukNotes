<!-- Настройки: форма строится по схеме с сервера, каждое изменение сразу сохраняется. -->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import type { SettingDef } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { ui } from "../lib/ui.svelte";

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (ui.settingsOpen && !dialog.open) {
      app.settingsError = null;
      dialog.showModal();
    } else if (!ui.settingsOpen && dialog.open) dialog.close();
  });

  const inGroup = (key: string) => app.schema?.settings.filter((s) => s.key.startsWith(`${key}.`)) ?? [];
  const save = (def: SettingDef, value: number | string | boolean) => app.saveSettings({ [def.key]: value });
</script>

<dialog class="settings" id="settings" bind:this={dialog} onclose={() => (ui.settingsOpen = false)}>
  <form method="dialog">
    <header>
      <h2>Настройки</h2>
      <button class="icon" value="close" aria-label="Закрыть"><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    {#each app.schema?.groups ?? [] as group (group.key)}
      <fieldset>
        <legend>{group.label}</legend>
        {#each inGroup(group.key) as def (def.key)}
          {@const value = app.settings[def.key]}
          <div class="setting">
            <label for="setting-{def.key}">{def.label}</label>
            {#if def.type === "bool"}
              <input
                id="setting-{def.key}"
                type="checkbox"
                checked={Boolean(value)}
                onchange={(e) => save(def, e.currentTarget.checked)}
              />
            {:else if def.type === "number"}
              <input
                id="setting-{def.key}"
                type="number"
                min={def.min}
                max={def.max}
                step={def.step}
                value={value}
                onchange={(e) => e.currentTarget.reportValidity() && save(def, Number(e.currentTarget.value))}
              />
            {:else}
              <select id="setting-{def.key}" value={value} onchange={(e) => save(def, e.currentTarget.value)}>
                {#each def.options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
              </select>
            {/if}
            {#if def.help}<div class="setting-help">{def.help}</div>{/if}
          </div>
        {/each}
      </fieldset>
    {/each}
    {#if app.settingsError}<p class="settings-error">{app.settingsError}</p>{/if}
  </form>
</dialog>
