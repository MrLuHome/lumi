export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** 是否运行在 Tauri 窗口内（否则是纯浏览器预览，图像管线不可用）。 */
export function isDesktopRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** macOS 与 Windows 的窗口装饰不同，布局类名由运行环境决定。 */
export function platformClass(): string {
  if (!isDesktopRuntime()) return "platform-web";
  return isMac ? "platform-macos" : "platform-windows";
}
