// The vault settings file with a JSON error (`.baluk/settings.json`): the warning in the settings window.

import type { SettingsProblem } from "./api";

/** Why changes are not kept and where to look; the core's `problem` of the vault settings. */
export function problemText(p: SettingsProblem): string {
  const at = p.line != null ? ` (строка ${p.line}${p.column != null ? `, столбец ${p.column}` : ""})` : "";
  return `Файл настроек хранилища содержит ошибку${at}. Изменения не сохраняются, пока её не исправят: ${p.path}`;
}
