// Прокручивается не документ, а колонка страницы под верхней панелью
// (`#page`): боковая и верхняя панели, оглавление стоят на месте. В окне
// (WebKitGTK) при прокрутке документа неподвижные слои ставятся заново на
// каждом кадре и при дробном масштабе экрана дрожат на пиксель.

/** Колонка, которая прокручивается; до её появления (экран выбора хранилища) — документ. */
export function scroller(): HTMLElement {
  return document.getElementById("page") ?? document.documentElement;
}

/** Прокрутка колонки сверху, px. */
export function scrollTop(): number {
  return scroller().scrollTop;
}

/** Прокрутить колонку к `y` (мгновенно). */
export function scrollToTop(y: number): void {
  scroller().scrollTo(0, y);
}

/** Докручено до конца (последние разделы до верха уже не доедут). */
export function atBottom(): boolean {
  const s = scroller();
  return s.clientHeight + s.scrollTop >= s.scrollHeight - 2;
}
