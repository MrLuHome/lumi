/**
 * 开发模式诊断日志。只记录阶段、计数、耗时与错误码，
 * 不记录照片内容、关键点坐标或完整个人路径（路径只保留文件名）。
 */
export function devLog(event: string, data: Record<string, unknown> = {}): void {
  const env = (import.meta as unknown as { env?: { DEV?: boolean } }).env;
  if (!env?.DEV) return;
  console.debug(`[lumi:${event}]`, data);
}

/** 只保留文件名，避免把绝对个人路径写进日志。 */
export function safeName(path: string): string {
  return path.split(/[\\/]/).pop() ?? "unknown";
}
