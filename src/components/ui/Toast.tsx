import { useEffect } from "react";
import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { useEditorStore } from "../../state/editorStore";

export function Toast() {
  const notice = useEditorStore((state) => state.notice);
  const dismiss = useEditorStore((state) => state.dismissNotice);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => dismiss(notice.id), notice.kind === "error" ? 6500 : 3200);
    return () => window.clearTimeout(timer);
  }, [notice, dismiss]);

  if (!notice) return null;

  const Icon = notice.kind === "error" ? CircleAlert : notice.kind === "success" ? CircleCheck : Info;

  return (
    <div className={`toast toast-${notice.kind}`} role="status">
      <Icon size={15} className="toast-icon" />
      <span className="toast-text">{notice.text}</span>
      <button type="button" className="toast-close" onClick={() => dismiss(notice.id)} aria-label="关闭提示">
        <X size={13} />
      </button>
    </div>
  );
}
