export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function formatZoom(zoom: number): string {
  const percent = zoom * 100;
  if (percent >= 100) return `${Math.round(percent)}%`;
  return `${percent.toFixed(percent >= 10 ? 0 : 1)}%`;
}

export function formatMegapixels(width: number, height: number): string {
  const mp = (width * height) / 1_000_000;
  return mp >= 10 ? `${mp.toFixed(1)} MP` : `${mp.toFixed(2)} MP`;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}
