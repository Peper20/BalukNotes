<!--
  Настройки: форма строится по схеме с сервера, каждое изменение сразу
  сохраняется — только для этого хранилища (настройки устройства — для всех).
  У изменённой настройки видно, откуда значение: только это хранилище или
  все хранилища; рядом — «для всех хранилищ», «как у всех», «по умолчанию».
-->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import type { SettingDef } from "../lib/api";
  import { settings } from "../lib/state";
  import { ui } from "../lib/ui.svelte";
  import { vault } from "../lib/vault";

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (ui.settingsOpen && !dialog.open) {
      settings.error = null;
      dialog.showModal();
    } else if (!ui.settingsOpen && dialog.open) dialog.close();
  });

  const inGroup = (key: string) => settings.schema?.settings.filter((s) => s.key.startsWith(`${key}.`)) ?? [];
  const save = (def: SettingDef, value: number | string | boolean) => settings.save({ [def.key]: value });
  /** Что меняется для всех хранилищ сразу: «тема, кегль текста». */
  const everywhere = $derived(
    (settings.schema?.settings ?? [])
      .filter((d) => d.shared)
      .map((d) => d.label.split(",")[0]!.toLowerCase())
      .join(", "),
  );
</script>

<dialog class="settings" id="settings" bind:this={dialog} onclose={() => (ui.settingsOpen = false)}>
  <form method="dialog">
    <header>
      <h2>Настройки</h2>
      <button class="icon" value="close" aria-label="Закрыть"><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    <p class="settings-scope">
      Изменения — только для хранилища «{vault()}». Для всех хранилищ сразу меняются: {everywhere} и настройки устройства.
    </p>
    {#each settings.schema?.groups ?? [] as group (group.key)}
      <fieldset>
        <legend>{group.label}</legend>
        {#each inGroup(group.key) as def (def.key)}
          {@const value = settings.values[def.key]}
          {@const source = settings.source(def.key)}
          <div class="setting" data-key={def.key} data-source={source}>
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
            {#if source !== "default" || def.shared}
              <div class="setting-source">
                {#if source === "own"}
                  <span class="setting-badge own">только в этом хранилище</span>
                  <button type="button" class="setting-action" onclick={() => settings.forAll(def.key)}>для всех хранилищ</button>
                  <button type="button" class="setting-action" onclick={() => settings.useShared(def.key)}>как у всех</button>
                {:else}
                  {#if source === "shared"}
                    <span class="setting-badge shared">{def.device ? "изменено на этом устройстве" : "изменено для всех хранилищ"}</span>
                    <button type="button" class="setting-action" onclick={() => settings.resetShared(def.key)}>по умолчанию</button>
                  {/if}
                  <!-- Тема и кегль меняются для всех: своё у хранилища — по выбору. -->
                  {#if def.shared}
                    <button type="button" class="setting-action" onclick={() => settings.onlyHere(def.key)}>только для этого хранилища</button>
                  {/if}
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </fieldset>
    {/each}
    {#if settings.error}<p class="settings-error">{settings.error}</p>{/if}
  </form>
</dialog>
