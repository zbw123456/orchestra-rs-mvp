export type Status = "queued" | "running" | "succeeded" | "failed" | "cancelled" | "timed_out" | "blocked" | "interrupted";
export type Strategy = "fifo" | "sjf" | "priority" | "aging";
export interface Task {
  id: string; title: string; prompt: string; provider: string; status: Status;
  estimate_ms: number; demo_duration_ms: number; timeout_ms: number; priority: number;
  depends_on: string[]; created_at_ms: number; started_at_ms: number | null;
  finished_at_ms: number | null; cancel_requested: boolean; output: string; detail: string; revision: number;
}
export interface Snapshot {
  schema_version: number; tasks: Task[]; running: number; capacity: number;
  strategy: Strategy; scheduling_source: string; providers: string[];
}
export interface Event { seq: number; task_id: string; status: Status; at_ms: number; detail: string }

export async function api<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  const response = await fetch(path, { method, headers: body === undefined ? {} : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(5000) });
  const text = await response.text();
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new Error(text || `HTTP ${response.status}`); }
  if (!response.ok) throw new Error((value as { error?: string }).error ?? `HTTP ${response.status}`);
  return value as T;
}
