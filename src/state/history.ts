export interface HistoryEntry<T> {
  state: T;
  label: string;
  at: number;
}

export interface HistoryStack<T> {
  past: HistoryEntry<T>[];
  future: HistoryEntry<T>[];
  limit: number;
}

export const DEFAULT_HISTORY_LIMIT = 100;

export function createHistory<T>(limit = DEFAULT_HISTORY_LIMIT): HistoryStack<T> {
  return { past: [], future: [], limit };
}

/** 记录一次变更前的状态；任何新变更都会清空重做栈。 */
export function pushHistory<T>(
  stack: HistoryStack<T> | undefined,
  before: T,
  label: string,
  now = Date.now(),
): HistoryStack<T> {
  const base = stack ?? createHistory<T>();
  const past = [...base.past, { state: before, label, at: now }];
  while (past.length > base.limit) past.shift();
  return { past, future: [], limit: base.limit };
}

export interface HistoryStep<T> {
  history: HistoryStack<T>;
  state: T;
  label: string;
}

export function undoHistory<T>(
  stack: HistoryStack<T> | undefined,
  current: T,
  now = Date.now(),
): HistoryStep<T> | null {
  if (!stack || stack.past.length === 0) return null;
  const past = [...stack.past];
  const entry = past.pop();
  if (!entry) return null;
  const future = [{ state: current, label: entry.label, at: now }, ...stack.future];
  return { history: { past, future, limit: stack.limit }, state: entry.state, label: entry.label };
}

export function redoHistory<T>(
  stack: HistoryStack<T> | undefined,
  current: T,
  now = Date.now(),
): HistoryStep<T> | null {
  if (!stack || stack.future.length === 0) return null;
  const [entry, ...rest] = stack.future;
  if (!entry) return null;
  const past = [...stack.past, { state: current, label: entry.label, at: now }];
  while (past.length > stack.limit) past.shift();
  return { history: { past, future: rest, limit: stack.limit }, state: entry.state, label: entry.label };
}

export function canUndo<T>(stack: HistoryStack<T> | undefined): boolean {
  return Boolean(stack && stack.past.length > 0);
}

export function canRedo<T>(stack: HistoryStack<T> | undefined): boolean {
  return Boolean(stack && stack.future.length > 0);
}
