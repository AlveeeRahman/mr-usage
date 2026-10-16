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
  updateSettings,
  wipeNow,
  type Point,
  type ProviderView,
  type Theme,
} from "./ipc";
import { meter, sparkline } from "./sparkline";

const views = new Map<string, ProviderView>();
const hints = new Map<string, string>();
const cardMessages = new Map<string, string>();
const confirmForget = new Set<string>();

const $ = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
};

const numberFmt = new Intl.NumberFormat();
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

function unitWord(view: ProviderView): string {
  return view.unit === "credits" ? "credits" : view.unit === "searches" ? "searches" : "of plan";
}

function amount(view: ProviderView, value: number): string {
  return view.unit === "percent" ? `${value}%` : numberFmt.format(value);
}

function minutes(secs: number): string {
  return secs % 3600 === 0 ? `${secs / 3600} h` : `${Math.round(secs / 60)} min`;
}

function severity(fraction: number): "ok" | "low" | "critical" {
  if (fraction < 0.05) return "critical";
  if (fraction < 0.2) return "low";
  return "ok";
}

function describePoint(view: ProviderView): (p: Point) => string {
  return (p) => (p.remaining === null ? `No reading at ${timeFmt.format(p.t)}` : `${amount(view, p.remaining)} left at ${timeFmt.format(p.t)}`);
}

function renderCard(view: ProviderView): HTMLElement {
  const card = h("article", "card");
  card.dataset.provider = view.id;

  const head = h("div", "card-head");
  head.append(h("h2", undefined, view.displayName));
  const latest = view.latest;
  const fraction = latest && latest.remaining !== null && latest.limit ? latest.remaining / latest.limit : null;
  const level = fraction === null ? "ok" : severity(fraction);
  if (level !== "ok") head.append(h("span", `badge ${level}`, level === "low" ? "Low" : "Nearly out"));
  card.append(head);

  if (latest && latest.remaining !== null) {
    const figure = h("p", "figure");
    figure.append(h("span", "value", amount(view, latest.remaining)));
    const of = latest.limit !== null && view.unit !== "percent"
      ? `of ${numberFmt.format(latest.limit)} ${unitWord(view)} left`
      : view.unit === "percent" ? "of plan left" : `${unitWord(view)} left`;
    figure.append(h("span", "of", of));
    card.append(figure);
    if (fraction !== null) card.append(meter(fraction, level));
  } else {
    card.append(h("p", "figure", view.error ? "No reading" : "Waiting for the first check…"));
  }

  const facts = h("dl", "facts");
  const fact = (label: string, value: string) => facts.append(h("dt", undefined, label), h("dd", undefined, value));
  // Billing dates are UTC (SerpApi sends a bare date, parsed as UTC midnight).
  // Formatting them in local time would show the previous day west of UTC.
  if (latest?.periodEndMs) fact(view.id === "serpapi" ? "Renews" : "Period ends", utcDateFmt.format(latest.periodEndMs));
  for (const [label, value] of latest?.extra ?? []) fact(label, localizeExtra(value));
  fact("Last reading", view.lastSuccessMs ? timeFmt.format(view.lastSuccessMs) : "None yet");
  fact("Checks every", minutes(view.pollIntervalSecs));
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
  const refreshBtn = h("button", "secondary", "Refresh");
  refreshBtn.type = "button";
  refreshBtn.addEventListener("click", () => void doRefresh(view.id, refreshBtn));
  const forgetBtn = h("button", "danger", confirmForget.has(view.id) ? "Click again to forget" : "Forget key");
  forgetBtn.type = "button";
  forgetBtn.addEventListener("click", () => void doForget(view.id));
  actions.append(refreshBtn, forgetBtn);
  card.append(actions);
  return card;
}

function renderAll(): void {
  const cards = $("cards");
  const shown = [...views.values()].filter((v) => v.hasKey || v.enabled);
  cards.replaceChildren(...shown.map(renderCard));
  $("empty").hidden = shown.length > 0;
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
  select.replaceChildren(
    ...[...views.values()].map((v) => {
      const option = h("option", undefined, v.hasKey ? `${v.displayName} (replace key)` : v.displayName);
      option.value = v.id;
      return option;
    }),
  );
  if (current && views.has(current)) select.value = current;
  showEndpoint();
}

function showEndpoint(): void {
  const view = views.get($<HTMLSelectElement>("add-provider").value);
  $("add-endpoint").textContent = view?.endpoint ?? "";
  $("add-docs").textContent = view?.docsUrl ?? "";
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

async function init(): Promise<void> {
  const keyInput = $<HTMLInputElement>("add-key");
  const interval = $<HTMLSelectElement>("poll-interval");
  const theme = $<HTMLSelectElement>("theme");

  $("add-provider").addEventListener("change", showEndpoint);
  $<HTMLFormElement>("add-form").addEventListener("submit", (ev) => {
    ev.preventDefault();
    const providerId = $<HTMLSelectElement>("add-provider").value;
    const key = keyInput.value;
    // Clear the field before the call so the key is not left on screen.
    keyInput.value = "";
    if (!key.trim()) {
      setStatus("add-status", "Paste a key first.", true);
      return;
    }
    const save = $<HTMLButtonElement>("add-save");
    save.disabled = true;
    setStatus("add-status", "Saving…");
    saveKey(providerId, key)
      .then((view) => {
        upsert(view);
        setStatus("add-status", `Saved. The first check for ${view.displayName} runs within 15 seconds.`);
        void loadHint(providerId);
      })
      .catch((err: unknown) => setStatus("add-status", errorText(err), true))
      .finally(() => {
        save.disabled = false;
      });
  });

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
