// Mr. Usage frontend. Text from vendors is inserted with textContent only.
import {
  errorText,
  forgetKey,
  getSettings,
  keyHint,
  listProviders,
  onUpdate,
  refresh,
  saveKey,
  setProviderConfig,
  updateLayout,
  updateSettings,
  wipeNow,
  type Category,
  type Point,
  type ProviderView,
  type Theme,
  type View,
} from "./ipc";
import { meter, sparkline } from "./sparkline";

const views = new Map<string, ProviderView>();
const hints = new Map<string, string>();
const cardMessages = new Map<string, string>();
const confirmForget = new Set<string>();

/** Which list is open, which cards are pinned, which are folded. Mirrors settings.json. */
const layout = { view: "all" as View, pinned: [] as string[], collapsed: new Set<string>() };

const CATEGORIES: { id: Category; label: string }[] = [
  { id: "search", label: "Search and scraping" },
  { id: "data", label: "Research and data" },
  { id: "llm", label: "LLM inference" },
  { id: "media", label: "Speech and media" },
  { id: "compute", label: "Compute" },
  { id: "quantum", label: "Quantum" },
  { id: "seo", label: "SEO and marketing" },
];

const $ = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
};

const numberFmt = new Intl.NumberFormat();
const moneyFmt: Record<"usd" | "cny" | "eur", Intl.NumberFormat> = {
  usd: new Intl.NumberFormat(undefined, { style: "currency", currency: "USD" }),
  cny: new Intl.NumberFormat(undefined, { style: "currency", currency: "CNY" }),
  eur: new Intl.NumberFormat(undefined, { style: "currency", currency: "EUR" }),
};
const timeFmt = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });
const utcDateFmt = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });

/** Format plain integers and ISO dates inside provider extras the same way as the rest of the card. */
function localizeExtra(value: string): string {
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    const ms = Date.parse(`${value}T00:00:00Z`);
    return Number.isNaN(ms) ? value : utcDateFmt.format(ms);
  }
  return value.replace(/\b\d{4,}\b/g, (n) => numberFmt.format(Number(n)));
}

function h<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label: string, className: string, onClick: (btn: HTMLButtonElement) => void): HTMLButtonElement {
  const btn = h("button", className, label);
  btn.type = "button";
  btn.addEventListener("click", () => onClick(btn));
  return btn;
}

/** The noun after a number, or "" for money, which needs none. */
function unitWord(view: ProviderView): string {
  switch (view.unit) {
    case "credits":
      return "credits";
    case "searches":
      return "searches";
    case "requests":
      return "requests";
    case "characters":
      return "characters";
    case "seconds":
    case "usd":
    case "cny":
    case "eur":
      return "";
    case "percent":
      return "of plan";
  }
}

function duration(secs: number): string {
  if (secs < 60) return `${secs} s`;
  const hours = Math.floor(secs / 3600);
  if (hours >= 48) {
    const days = Math.round(hours / 24);
    return `${numberFmt.format(days)} days`;
  }
  const mins = Math.round((secs % 3600) / 60);
  if (hours === 0) return `${mins} min`;
  return mins === 0 ? `${hours} h` : `${hours} h ${mins} min`;
}

function amount(view: ProviderView, value: number): string {
  switch (view.unit) {
    case "percent":
      return `${value}%`;
    case "usd":
    case "cny":
    case "eur":
      return moneyFmt[view.unit].format(value / 100);
    case "seconds":
      return duration(value);
    default:
      return numberFmt.format(value);
  }
}

/** "of 1,000 credits left", "of $100.00 left", "credits left", "of plan left". */
function leftPhrase(view: ProviderView, limit: number | null): string {
  const word = unitWord(view);
  if (view.unit === "percent") return "of plan left";
  if (limit !== null) return `of ${amount(view, limit)}${word ? ` ${word}` : ""} left`;
  return word ? `${word} left` : "left";
}

function minutes(secs: number): string {
  return secs % 3600 === 0 ? `${secs / 3600} h` : `${Math.round(secs / 60)} min`;
}

type Level = "ok" | "low" | "critical";

function severity(fraction: number): Level {
  if (fraction < 0.05) return "critical";
  if (fraction < 0.2) return "low";
  return "ok";
}

/** Spend against a cap reads the other way round: full is bad. */
function spendSeverity(fraction: number): Level {
  if (fraction >= 0.95) return "critical";
  if (fraction >= 0.8) return "low";
  return "ok";
}

function describePoint(view: ProviderView): (p: Point) => string {
  return (p) => (p.remaining === null ? `No reading at ${timeFmt.format(p.t)}` : `${amount(view, p.remaining)} left at ${timeFmt.format(p.t)}`);
}

/** How fast a balance is falling, from today's readings or the vendor's own rate. */
function pace(view: ProviderView): { text: string; daysLeft: number | null } | null {
  const latest = view.latest;
  if (!latest || latest.remaining === null) return null;
  if (latest.spendPerHour !== null && latest.spendPerHour > 0) {
    const hours = latest.remaining / latest.spendPerHour;
    return { text: `${amount(view, latest.spendPerHour)} an hour at the current rate, about ${duration(Math.round(hours * 3600))} left`, daysLeft: hours / 24 };
  }
  const data = view.history.filter((p): p is { t: number; remaining: number } => p.remaining !== null);
  if (data.length < 2) return null;
  const first = data[0]!;
  const last = data[data.length - 1]!;
  const hours = (last.t - first.t) / 3_600_000;
  if (hours < 1 / 3) return null;
  const drop = first.remaining - last.remaining;
  if (drop <= 0) return { text: `Unchanged since ${timeFmt.format(first.t)}`, daysLeft: null };
  const perHour = drop / hours;
  const daysLeft = last.remaining / (perHour * 24);
  const horizon = daysLeft < 1 ? "under a day" : `about ${numberFmt.format(Math.round(daysLeft))} day${Math.round(daysLeft) === 1 ? "" : "s"}`;
  return { text: `Down ${amount(view, drop)} since ${timeFmt.format(first.t)}, ${horizon} at this pace`, daysLeft };
}

function isPinned(id: string): boolean {
  return layout.pinned.includes(id);
}

function saveLayout(): void {
  updateLayout(layout.view, layout.pinned, [...layout.collapsed]).catch(() => {
    // The window keeps its state; only the file write failed, and the
    // settings panel reports those.
  });
}

function togglePin(id: string): void {
  if (isPinned(id)) layout.pinned = layout.pinned.filter((p) => p !== id);
  else layout.pinned = [...layout.pinned, id];
  saveLayout();
  renderAll();
}

function toggleCollapsed(id: string): void {
  if (layout.collapsed.has(id)) layout.collapsed.delete(id);
  else layout.collapsed.add(id);
  saveLayout();
  renderAll();
}

function setView(view: View): void {
  layout.view = view;
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
    tab.setAttribute("aria-selected", tab.dataset.view === view ? "true" : "false");
  }
  saveLayout();
  renderAll();
}

/** Open the add panel with this provider selected and the key field focused. */
function startAdding(id: string): void {
  const panel = $<HTMLDetailsElement>("add-panel");
  panel.open = true;
  const select = $<HTMLSelectElement>("add-provider");
  select.value = id;
  showEndpoint();
  panel.scrollIntoView({ block: "start" });
  $<HTMLInputElement>("add-key").focus();
}

function renderCard(view: ProviderView): HTMLElement {
  const card = h("article", "card");
  card.dataset.provider = view.id;
  const latest = view.latest;
  const expanded = view.hasKey && !layout.collapsed.has(view.id);
  card.classList.toggle("keyed", view.hasKey);
  card.classList.toggle("compact", !expanded);

  // Which of the three shapes this reading takes.
  const quota = latest !== null && latest.remaining !== null && latest.limit !== null && latest.limit > 0;
  const balance = latest !== null && latest.remaining !== null && !quota;
  const spend = latest !== null && latest.remaining === null && latest.spent !== null;
  const paceInfo = balance ? pace(view) : null;

  let level: Level = "ok";
  let fraction: number | null = null;
  if (quota) {
    fraction = latest.remaining! / latest.limit!;
    level = severity(fraction);
  } else if (spend && latest.limit !== null && latest.limit > 0) {
    fraction = latest.spent! / latest.limit;
    level = spendSeverity(fraction);
  } else if (balance && paceInfo?.daysLeft !== null && paceInfo?.daysLeft !== undefined) {
    level = paceInfo.daysLeft < 1 ? "critical" : paceInfo.daysLeft < 3 ? "low" : "ok";
  }

  const head = h("div", "card-head");
  head.append(h("h2", undefined, view.displayName));
  if (level !== "ok") head.append(h("span", `badge ${level}`, level === "low" ? "Low" : "Nearly out"));
  const controls = h("div", "card-controls");
  const pinBtn = button(isPinned(view.id) ? "Unpin" : "Pin", "icon-button", () => togglePin(view.id));
  pinBtn.setAttribute("aria-pressed", isPinned(view.id) ? "true" : "false");
  pinBtn.title = isPinned(view.id) ? "Remove from Pinned" : "Show on Pinned";
  controls.append(pinBtn);
  if (view.hasKey) {
    const fold = button(expanded ? "Less" : "More", "icon-button", () => toggleCollapsed(view.id));
    fold.setAttribute("aria-expanded", expanded ? "true" : "false");
    fold.title = expanded ? "Fold this card to its headline" : "Show details";
    controls.append(fold);
  }
  head.append(controls);
  card.append(head);

  if (!view.hasKey) {
    card.append(h("p", "figure muted", "No key saved"));
    const actions = h("div", "actions");
    actions.append(button("Add key", "secondary", () => startAdding(view.id)));
    card.append(actions);
    return card;
  }

  if (quota || balance) {
    const figure = h("p", "figure");
    figure.append(h("span", "value", amount(view, latest!.remaining!)));
    figure.append(h("span", "of", leftPhrase(view, quota ? latest!.limit : null)));
    card.append(figure);
    if (fraction !== null) card.append(meter(fraction, level));
    if (paceInfo) card.append(h("p", "pace", paceInfo.text));
  } else if (spend) {
    const figure = h("p", "figure");
    figure.append(h("span", "value", amount(view, latest!.spent!)));
    const cap = latest!.limit !== null && latest!.limit > 0 ? `of ${amount(view, latest!.limit)} cap, spent this period` : "spent this period";
    figure.append(h("span", "of", cap));
    card.append(figure);
    if (fraction !== null) card.append(meter(fraction, level));
  } else if (latest) {
    // A reading with no count: this vendor reports neither a balance nor spend.
    card.append(h("p", "figure", "No remaining count on this plan"));
  } else {
    card.append(h("p", "figure", view.error ? "No reading" : "Waiting for the first check…"));
  }

  if (!expanded) {
    if (view.error) {
      const err = h("p", "error", view.error.message);
      err.setAttribute("role", "alert");
      card.append(err);
    }
    return card;
  }

  const facts = h("dl", "facts");
  const fact = (label: string, value: string) => facts.append(h("dt", undefined, label), h("dd", undefined, value));
  // Billing dates are UTC (SerpApi sends a bare date, parsed as UTC midnight).
  // Formatting them in local time would show the previous day west of UTC.
  if (latest?.periodEndMs) fact(view.id === "serpapi" ? "Renews" : "Period ends", utcDateFmt.format(latest.periodEndMs));
  if (latest && !spend && latest.spent !== null) fact("Spent this period", amount(view, latest.spent));
  if (latest && latest.spendPerHour !== null && !balance) fact("Spend per hour", amount(view, latest.spendPerHour));
  for (const [label, value] of latest?.extra ?? []) fact(label, localizeExtra(value));
  fact("Last reading", view.lastSuccessMs ? timeFmt.format(view.lastSuccessMs) : "None yet");
  fact("Checks every", minutes(view.pollIntervalSecs));
  if (view.configField && view.config) fact(view.configField.label, view.config);
  const hint = hints.get(view.id);
  if (hint) fact("Key", hint);
  card.append(facts);

  const readout = h("p", "readout");
  const spark = sparkline(view.history, describePoint(view), readout);
  if (spark) card.append(spark, readout);

  if (view.note) card.append(h("p", "note", view.note));
  if (view.error) {
    const err = h("p", "error", view.error.message);
    err.setAttribute("role", "alert");
    card.append(err);
  }
  const message = cardMessages.get(view.id);
  if (message) card.append(h("p", "note", message));

  const actions = h("div", "actions");
  actions.append(button("Refresh", "secondary", (btn) => void doRefresh(view.id, btn)));
  actions.append(button(confirmForget.has(view.id) ? "Click again to forget" : "Forget key", "danger", () => void doForget(view.id)));
  card.append(actions);
  return card;
}

function grid(items: ProviderView[]): HTMLElement {
  const list = h("div", "grid");
  list.append(...items.map(renderCard));
  return list;
}

/** Keyed cards first, otherwise the registry's display order. */
function keyedFirst(items: ProviderView[]): ProviderView[] {
  return [...items.filter((v) => v.hasKey), ...items.filter((v) => !v.hasKey)];
}

function renderAll(): void {
  const cards = $("cards");
  const empty = $("empty");
  if (layout.view === "pinned") {
    const shown = layout.pinned.map((id) => views.get(id)).filter((v): v is ProviderView => v !== undefined);
    cards.replaceChildren(grid(shown));
    empty.textContent = "Nothing pinned yet. Open All APIs and press Pin on a card.";
    empty.hidden = shown.length > 0;
  } else {
    const groups: HTMLElement[] = [];
    for (const { id, label } of CATEGORIES) {
      const members = [...views.values()].filter((v) => v.category === id);
      if (members.length === 0) continue;
      const section = h("section", "group");
      section.setAttribute("aria-label", label);
      const heading = h("h2", "group-title", label);
      const keyed = members.filter((v) => v.hasKey).length;
      heading.append(h("span", "group-count", keyed ? `${keyed} of ${members.length} with a key` : `${members.length}`));
      section.append(heading, grid(keyedFirst(members)));
      groups.push(section);
    }
    cards.replaceChildren(...groups);
    empty.textContent = "";
    empty.hidden = true;
  }
  renderAddOptions();
}

function upsert(view: ProviderView): void {
  views.set(view.id, view);
  if (!view.hasKey) hints.delete(view.id);
  renderAll();
}

async function loadHint(id: string): Promise<void> {
  try {
    const hint = await keyHint(id);
    if (hint) hints.set(id, hint);
    else hints.delete(id);
    renderAll();
  } catch {
    // The card already shows any keystore error from the poller.
  }
}

function flash(id: string, message: string): void {
  cardMessages.set(id, message);
  renderAll();
  window.setTimeout(() => {
    if (cardMessages.get(id) === message) {
      cardMessages.delete(id);
      renderAll();
    }
  }, 6000);
}

async function doRefresh(id: string, button: HTMLButtonElement): Promise<void> {
  button.disabled = true;
  try {
    upsert(await refresh(id));
  } catch (err) {
    flash(id, errorText(err));
  } finally {
    button.disabled = false;
  }
}

async function doForget(id: string): Promise<void> {
  if (!confirmForget.has(id)) {
    confirmForget.add(id);
    renderAll();
    window.setTimeout(() => {
      confirmForget.delete(id);
      renderAll();
    }, 4000);
    return;
  }
  confirmForget.delete(id);
  try {
    upsert(await forgetKey(id));
  } catch (err) {
    flash(id, errorText(err));
  }
}

function renderAddOptions(): void {
  const select = $<HTMLSelectElement>("add-provider");
  const current = select.value;
  const groups = CATEGORIES.map(({ id, label }) => {
    const group = h("optgroup");
    group.label = label;
    for (const v of views.values()) {
      if (v.category !== id) continue;
      const option = h("option", undefined, v.hasKey ? `${v.displayName} (replace key)` : v.displayName);
      option.value = v.id;
      group.append(option);
    }
    return group;
  }).filter((g) => g.childElementCount > 0);
  select.replaceChildren(...groups);
  if (current && views.has(current)) select.value = current;
  showEndpoint();
}

function showEndpoint(): void {
  const view = views.get($<HTMLSelectElement>("add-provider").value);
  $("add-endpoint").textContent = view?.endpoint ?? "";
  $("add-docs").textContent = view?.docsUrl ?? "";
  const row = $("add-config-row");
  const field = view?.configField ?? null;
  row.hidden = field === null;
  if (field) {
    $("add-config-label").textContent = field.label;
    $("add-config-hint").textContent = field.hint;
    const input = $<HTMLInputElement>("add-config");
    input.placeholder = field.placeholder;
    input.value = view?.config ?? "";
  }
}

function setStatus(id: string, text: string, bad = false): void {
  const node = $(id);
  node.textContent = text;
  node.classList.toggle("bad", bad);
}

function applyTheme(theme: Theme): void {
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", theme);
}

async function submitAdd(): Promise<void> {
  const keyInput = $<HTMLInputElement>("add-key");
  const providerId = $<HTMLSelectElement>("add-provider").value;
  const view = views.get(providerId);
  const key = keyInput.value;
  // Clear the field before any call so the key is not left on screen.
  keyInput.value = "";
  const configInput = $<HTMLInputElement>("add-config");
  const wantsConfig = view?.configField !== null && view?.configField !== undefined;
  const config = wantsConfig ? configInput.value.trim() : "";
  const configChanged = wantsConfig && config !== (view?.config ?? "");

  if (!key.trim() && !configChanged) {
    setStatus("add-status", "Paste a key first.", true);
    return;
  }
  const save = $<HTMLButtonElement>("add-save");
  save.disabled = true;
  setStatus("add-status", "Saving…");
  try {
    if (configChanged) upsert(await setProviderConfig(providerId, config || null));
    if (key.trim()) {
      const saved = await saveKey(providerId, key);
      upsert(saved);
      setStatus("add-status", `Saved. The first check for ${saved.displayName} runs within 15 seconds.`);
      void loadHint(providerId);
    } else {
      setStatus("add-status", `Saved the ${view?.configField?.label ?? "identifier"} for ${view?.displayName ?? providerId}.`);
    }
  } catch (err) {
    setStatus("add-status", errorText(err), true);
  } finally {
    save.disabled = false;
  }
}

async function init(): Promise<void> {
  const interval = $<HTMLSelectElement>("poll-interval");
  const theme = $<HTMLSelectElement>("theme");

  $("add-provider").addEventListener("change", showEndpoint);
  $<HTMLFormElement>("add-form").addEventListener("submit", (ev) => {
    ev.preventDefault();
    void submitAdd();
  });
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
    tab.addEventListener("click", () => setView(tab.dataset.view === "pinned" ? "pinned" : "all"));
  }

  const pushSettings = () => {
    updateSettings(Number(interval.value), theme.value as Theme)
      .then((s) => {
        applyTheme(s.settings.theme);
        setStatus("settings-status", "Saved.");
      })
      .catch((err: unknown) => setStatus("settings-status", errorText(err), true));
  };
  interval.addEventListener("change", pushSettings);
  theme.addEventListener("change", pushSettings);
  $("wipe-now").addEventListener("click", () => {
    wipeNow()
      .then(() => setStatus("settings-status", "History wiped."))
      .catch((err: unknown) => setStatus("settings-status", errorText(err), true));
  });

  await onUpdate(upsert);

  const settings = await getSettings();
  applyTheme(settings.settings.theme);
  theme.value = settings.settings.theme;
  const secs = String(settings.settings.pollIntervalSecs);
  if (![...interval.options].some((o) => o.value === secs)) {
    const custom = h("option", undefined, minutes(settings.settings.pollIntervalSecs));
    custom.value = secs;
    interval.append(custom);
  }
  interval.value = secs;
  for (const node of document.querySelectorAll(".store-label")) node.textContent = settings.storeLabel;
  $("footer").textContent = `Keys stored in ${settings.storeLabel} under the service name "mr-usage".`;

  layout.pinned = settings.settings.pinned;
  layout.collapsed = new Set(settings.settings.collapsed);
  layout.view = settings.settings.view;
  for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
    tab.setAttribute("aria-selected", tab.dataset.view === layout.view ? "true" : "false");
  }

  for (const view of await listProviders()) views.set(view.id, view);
  renderAll();
  if (![...views.values()].some((v) => v.hasKey)) $<HTMLDetailsElement>("add-panel").open = true;
  for (const view of views.values()) if (view.hasKey) void loadHint(view.id);
}

init().catch((err: unknown) => {
  const banner = $("banner");
  banner.textContent = `Mr. Usage could not start: ${errorText(err)}`;
  banner.hidden = false;
});
