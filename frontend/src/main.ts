import "./style.css";
import { api, type Event, type Snapshot, type Strategy, type Task } from "./api";

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <header><a class="brand" href="/" aria-label="Orchestra home"><span class="mark">▥</span> orchestra<span class="lab">LAB</span></a><div class="connection" id="connection">Connecting…</div></header>
  <main>
    <section class="intro"><div><p class="eyebrow">LOCAL AGENT WORKSPACE</p><h1>Execution desk<span>.</span></h1><p>Give work a clear path from queue to result.</p></div><span class="scope">Experimental MVP · v0.1</span></section>
    <div id="notice" role="alert" hidden></div>
    <section class="metrics" aria-label="Task overview"><div><span>Queued</span><strong id="queued">0</strong></div><div><span>Workers in use</span><strong id="running">0 / 2</strong></div><div><span>Completed</span><strong id="completed">0</strong></div><div><span>Needs attention</span><strong id="failed">0</strong></div></section>
    <div class="workspace"><aside>
      <section class="panel"><div class="panel-heading"><h2>New task</h2><span class="number">01</span></div>
        <form id="task-form"><label>Task name<input name="title" required maxlength="200" placeholder="Review the API contract"></label>
          <label>Prompt<textarea name="prompt" maxlength="16000" rows="3" placeholder="Describe the work for your agent…"></textarea></label>
          <div class="pair"><label>Provider<select name="provider" id="provider"><option value="demo">demo</option></select></label><label>Priority<select name="priority"><option value="0">0 · Normal</option><option value="5">5 · High</option><option value="10">10 · Urgent</option></select></label></div>
          <div class="pair"><label>Estimate (ms)<input name="estimate_ms" type="number" min="1" max="3600000" value="3000" required></label><label>Demo time (ms)<input name="demo_duration_ms" type="number" min="1" max="3600000" value="3000" required></label></div>
          <div class="pair"><label>Timeout (ms)<input name="timeout_ms" type="number" min="10" max="3600000" value="30000" required></label><label>Depends on<select name="dependency" id="dependency"><option value="">No dependency</option></select></label></div>
          <label class="check"><input type="checkbox" name="demo_fail"> Simulate a failure</label>
          <button class="primary" type="submit">Add to queue <span>↗</span></button><p class="hint">Demo runs a local worker process. It does not call a language model.</p>
        </form>
      </section>
      <section class="panel policy"><div class="panel-heading"><h2>Scheduling policy</h2><span class="number">02</span></div><label class="sr-only" for="strategy">Scheduling policy</label><select id="strategy"><option value="aging">Aging · priority + waiting time</option><option value="fifo">FIFO · first in, first out</option><option value="sjf">SJF · shortest estimate first</option><option value="priority">Priority · urgent work first</option></select><p id="policy-description" class="hint"></p><div class="source" id="source">Waiting for a decision</div></section>
    </aside><section class="panel queue"><div class="panel-heading"><div><p class="eyebrow">WORK IN MOTION</p><h2>Task queue</h2></div><label class="sr-only" for="filter">Filter tasks</label><select id="filter"><option value="all">All tasks</option><option value="active">Active</option><option value="succeeded">Completed</option><option value="attention">Needs attention</option></select></div><div id="tasks"></div></section></div>
    <section class="panel activity"><div class="panel-heading"><h2>Recent activity</h2><span class="hint">Persistent transition journal</span></div><ol id="events"><li>No events yet.</li></ol></section>
    <footer><span>Inspired by <a href="https://github.com/wayyoungboy/Orchestra" target="_blank" rel="noreferrer">wayyoungboy/Orchestra</a> · MIT</span><span>Rust runtime / Python policies / TypeScript UI</span></footer>
  </main>`;

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const terminal = (t: Task) => t.status !== "queued" && t.status !== "running";
const attention = (t: Task) => ["failed", "timed_out", "blocked", "interrupted"].includes(t.status);
const descriptions: Record<Strategy, string> = {
  fifo: "Runs the oldest ready task first. Rust also uses this if the policy service is unavailable.",
  sjf: "Prefers shorter estimated work. Under sustained arrivals, long tasks may wait longer.",
  priority: "Prefers higher priorities, with oldest-first ties. Low-priority work may wait.",
  aging: "Combines priority and wait / estimated duration. A transparent heuristic, with no trained model.",
};
let snapshot: Snapshot | undefined;
let events: Event[] = [];
let after = 0;
let strategyBusy = false;
let pendingRefresh: Promise<void> | undefined;
const openTasks = new Set<string>();

function notice(message: string) { $("#notice").textContent = message; $("#notice").hidden = !message; }
function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag); element.textContent = text; element.className = className; return element;
}
function options(select: HTMLSelectElement, values: { value: string; text: string }[]) {
  if (JSON.stringify(values) === select.dataset.values) return;
  const chosen = select.value;
  select.replaceChildren(...values.map(v => { const option = node("option", v.text); option.value = v.value; return option; }));
  if (values.some(v => v.value === chosen)) select.value = chosen;
  select.dataset.values = JSON.stringify(values);
}
function render() {
  if (!snapshot) return;
  const tasks = snapshot.tasks;
  $("#queued").textContent = String(tasks.filter(t => t.status === "queued").length);
  $("#running").textContent = `${snapshot.running} / ${snapshot.capacity}`;
  $("#completed").textContent = String(tasks.filter(t => t.status === "succeeded").length);
  $("#failed").textContent = String(tasks.filter(attention).length);
  if (!strategyBusy && document.activeElement !== $("#strategy")) $<HTMLSelectElement>("#strategy").value = snapshot.strategy;
  $("#policy-description").textContent = descriptions[snapshot.strategy];
  $("#source").textContent = snapshot.scheduling_source;
  $("#source").classList.toggle("fallback", snapshot.scheduling_source.startsWith("fallback"));
  options($("#provider"), snapshot.providers.map(p => ({ value: p, text: p })));
  options($("#dependency"), [{ value: "", text: "No dependency" }, ...tasks.map(t => ({ value: t.id, text: `${t.title} · ${t.status}` }))]);
  const filter = $<HTMLSelectElement>("#filter").value;
  const visible = tasks.filter(t => filter === "all" || (filter === "active" && !terminal(t)) || (filter === "succeeded" && t.status === "succeeded") || (filter === "attention" && attention(t)));
  const list = $("#tasks");
  list.replaceChildren();
  if (!visible.length) {
    const empty = node("div", "", "empty"); empty.append(node("span", "▥", "empty-icon"), node("h3", "A little room for your next idea."), node("p", "Add a task to watch it move through the queue.")); list.append(empty);
  }
  for (const task of visible) {
    const row = node("article", "", "task");
    const heading = node("div", "", "task-heading");
    heading.append(node("h3", task.title), node("span", task.cancel_requested && task.status === "running" ? "cancelling" : task.status.replaceAll("_", " "), `badge ${task.status}`));
    const meta = node("p", `${task.provider} · Priority ${task.priority} · Estimate ${(task.estimate_ms / 1000).toFixed(1)}s${task.depends_on.length ? ` · ${task.depends_on.length} dependency` : ""}`, "task-meta");
    const details = node("details"); details.open = openTasks.has(task.id);
    details.addEventListener("toggle", () => { if (details.open) openTasks.add(task.id); else openTasks.delete(task.id); });
    details.append(node("summary", "Details & output"), node("p", task.detail), node("p", task.prompt || "No prompt provided.", "prompt"), node("code", task.id));
    details.append(node("pre", task.output || "Output will appear after the process finishes."));
    row.append(heading, meta, details);
    if (!terminal(task)) {
      const cancel = node("button", task.cancel_requested ? "Cancelling…" : "Cancel", "cancel"); cancel.disabled = task.cancel_requested;
      cancel.onclick = async () => { cancel.disabled = true; try { await api(`/api/tasks/${task.id}/cancel`, "POST"); await refresh(); } catch (e) { notice(String(e)); cancel.disabled = false; } };
      row.append(cancel);
    }
    list.append(row);
  }
}

function refresh(): Promise<void> {
  // Polling and user actions share one read so event pages cannot be appended twice.
  pendingRefresh ??= readState().finally(() => { pendingRefresh = undefined; });
  return pendingRefresh;
}
async function readState() {
  try {
    const [state, incoming] = await Promise.all([api<Snapshot>("/api/state"), api<Event[]>(`/api/events?after=${after}`)]);
    snapshot = state;
    $("#connection").textContent = "● Local core connected";
    $("#connection").classList.remove("offline");
    if (incoming.length) {
      after = incoming[incoming.length - 1].seq;
      events = [...events, ...incoming].slice(-12);
      $("#events").replaceChildren(...events.slice().reverse().map(e => {
        const li = node("li"); li.append(node("time", new Date(e.at_ms).toLocaleTimeString()), node("span", e.status.replaceAll("_", " "), `event-status ${e.status}`), node("span", `${e.task_id.slice(0, 8)} · ${e.detail}`)); return li;
      }));
    }
    render();
  } catch {
    $("#connection").textContent = "● Core unavailable · reconnecting";
    $("#connection").classList.add("offline");
  }
}

$("#task-form").addEventListener("submit", async event => {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const data = new FormData(form);
  const button = form.querySelector<HTMLButtonElement>("button[type=submit]")!;
  button.disabled = true; notice("");
  try {
    await api("/api/tasks", "POST", { title: data.get("title"), prompt: data.get("prompt"), provider: data.get("provider"),
      priority: Number(data.get("priority")), estimate_ms: Number(data.get("estimate_ms")), demo_duration_ms: Number(data.get("demo_duration_ms")),
      timeout_ms: Number(data.get("timeout_ms")), demo_fail: data.has("demo_fail"), depends_on: data.get("dependency") ? [data.get("dependency")] : [] });
    (form.elements.namedItem("title") as HTMLInputElement).value = "";
    await refresh();
  } catch (e) { notice(String(e)); } finally { button.disabled = false; }
});
$("#strategy").addEventListener("change", async event => {
  strategyBusy = true; notice("");
  try { await api("/api/strategy", "PUT", { strategy: (event.target as HTMLSelectElement).value }); await refresh(); }
  catch (e) { notice(String(e)); } finally { strategyBusy = false; }
});
$("#filter").addEventListener("change", render);
async function poll() { await refresh(); window.setTimeout(poll, 1000); }
void poll();
