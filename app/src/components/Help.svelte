<!-- Справка: горячие клавиши по группам (из реестра команд) и приёмы палитры. -->
<script lang="ts">
  import X from "@lucide/svelte/icons/x";
  import { commands } from "../lib/commands.svelte";
  import { ui } from "../lib/ui.svelte";

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (ui.helpOpen && !dialog.open) dialog.showModal();
    else if (!ui.helpOpen && dialog.open) dialog.close();
  });

  const groups = $derived(
    Map.groupBy(
      commands().filter((c) => c.keys?.length && !c.id.startsWith("tab-")),
      (c) => c.group,
    ),
  );
</script>

<dialog class="settings help" bind:this={dialog} onclose={() => (ui.helpOpen = false)} aria-label="Горячие клавиши">
  <form method="dialog">
    <header>
      <h2>Горячие клавиши</h2>
      <button class="icon" value="close" aria-label="Закрыть"><X size={18} strokeWidth={1.75} aria-hidden="true" /></button>
    </header>
    <div class="help-body">
      {#each groups as [group, list] (group)}
        <section>
          <h3>{group}</h3>
          <dl>
            {#each list as c (c.id)}
              <dt>{#each c.keys! as k, i}{#if i}<span class="or"> или </span>{/if}<kbd>{k.label}</kbd>{/each}</dt>
              <dd>{c.title}</dd>
            {/each}
            {#if group === "Вкладки"}<dt><kbd>Alt+1…9</kbd></dt><dd>Вкладка по номеру</dd>{/if}
          </dl>
        </section>
      {/each}
      <section>
        <h3>Мышь и палитра</h3>
        <dl>
          <dt><kbd>Ctrl</kbd>+клик</dt><dd>Открыть ссылку в новой вкладке (или средней кнопкой)</dd>
          <dt><kbd>&gt;</kbd> <kbd>/</kbd> <kbd>#</kbd></dt><dd>В палитре: команды, поиск по тексту, теги</dd>
          <dt>наведение</dt><dd>На ссылку в заметке — превью заметки или раздела</dd>
        </dl>
      </section>
    </div>
  </form>
</dialog>
