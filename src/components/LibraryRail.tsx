import { ImagePlus, X } from "lucide-react";
import type { PhotoAsset } from "../types";
import { useEditorStore } from "../state/editorStore";
import { formatFileSize } from "../lib/format";
import { isNeutral } from "../state/params";

interface LibraryRailProps {
  photos: PhotoAsset[];
  activeId: string | null;
  onImport: () => void;
}

export function LibraryRail({ photos, activeId, onImport }: LibraryRailProps) {
  const setActive = useEditorStore((state) => state.setActive);
  const removePhoto = useEditorStore((state) => state.removePhoto);
  const paramsMap = useEditorStore((state) => state.params);

  return (
    <aside className="rail">
      <div className="rail-head">
        <span className="eyebrow">图库</span>
        <span className="rail-count">{photos.length.toString().padStart(2, "0")}</span>
        <button type="button" className="rail-add" onClick={onImport} title="导入照片">
          <ImagePlus size={14} />
        </button>
      </div>

      <div className="rail-list">
        {photos.map((photo) => {
          const photoParams = paramsMap[photo.id];
          const edited = photoParams ? !isNeutral(photoParams) : false;
          return (
            <div
              key={photo.id}
              className={`rail-item ${photo.id === activeId ? "is-active" : ""}`}
              role="button"
              tabIndex={0}
              onClick={() => setActive(photo.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setActive(photo.id);
                }
              }}
            >
              <span className="rail-thumb">
                <img src={photo.thumbUrl} alt={photo.fileName} draggable={false} />
              </span>
              <span className="rail-meta">
                <span className="rail-name">{photo.fileName}</span>
                <span className="rail-sub">
                  {photo.width} × {photo.height}
                  <span className="rail-dot-sep">·</span>
                  {formatFileSize(photo.fileSize)}
                </span>
              </span>
              {edited && <span className="rail-edited" title="已有调整" />}
              <button
                type="button"
                className="rail-remove"
                title="从图库移除"
                onClick={(event) => {
                  event.stopPropagation();
                  removePhoto(photo.id);
                }}
              >
                <X size={12} />
              </button>
            </div>
          );
        })}

        {photos.length === 0 && (
          <div className="rail-empty">
            <p>图库为空</p>
            <span>导入 JPG / PNG / WebP 开始修图</span>
          </div>
        )}
      </div>

      <div className="rail-foot">
        <span>支持 JPG · PNG · WebP</span>
      </div>
    </aside>
  );
}
