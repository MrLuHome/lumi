import { useMemo, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";
import type { CurveChannel, CurvePoint } from "../types";
import { CURVE_CHANNELS } from "../state/params";
import { useEditorStore } from "../state/editorStore";

const WIDTH = 240;
const HEIGHT = 150;
const PAD = 12;

const IDENTITY: CurvePoint[] = [
  { x: 0, y: 0 },
  { x: 1, y: 1 },
];

function toScreen(point: CurvePoint) {
  return {
    x: PAD + point.x * (WIDTH - PAD * 2),
    y: HEIGHT - PAD - point.y * (HEIGHT - PAD * 2),
  };
}

function toValue(clientX: number, clientY: number, rect: DOMRect): CurvePoint {
  const x = (clientX - rect.left - PAD) / (WIDTH - PAD * 2);
  const y = 1 - (clientY - rect.top - PAD) / (HEIGHT - PAD * 2);
  return { x: Math.min(1, Math.max(0, x)), y: Math.min(1, Math.max(0, y)) };
}

/** 曲线编辑器：拖动控制点实时调色，双击删除，空白处单击新增。 */
export function CurveEditor() {
  const curves = useEditorStore((state) => state.params[state.activeId ?? ""]?.curves);
  const setCurve = useEditorStore((state) => state.setCurve);
  const resetCurve = useEditorStore((state) => state.resetCurve);
  const [channel, setChannel] = useState<CurveChannel>("rgb");
  const dragRef = useRef<number | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);

  const stored = curves?.[channel] ?? [];
  const points = stored.length >= 2 ? stored : IDENTITY;
  const isDirty = stored.length >= 2;

  const path = useMemo(() => {
    return points
      .map((point, index) => {
        const { x, y } = toScreen(point);
        return `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(" ");
  }, [points]);

  const activeChannel = CURVE_CHANNELS.find((item) => item.key === channel) ?? CURVE_CHANNELS[0]!;

  const apply = (next: CurvePoint[]) => {
    setCurve(channel, [...next].sort((a, b) => a.x - b.x));
  };

  const movePoint = (index: number, value: CurvePoint) => {
    const next = points.map((point) => ({ ...point }));
    let x = value.x;
    if (index === 0) x = 0;
    else if (index === points.length - 1) x = 1;
    else {
      const previous = points[index - 1]?.x ?? 0;
      const following = points[index + 1]?.x ?? 1;
      x = Math.min(following - 0.02, Math.max(previous + 0.02, x));
    }
    next[index] = { x, y: value.y };
    apply(next);
  };

  const onPointerDown = (index: number) => (event: React.PointerEvent) => {
    event.stopPropagation();
    dragRef.current = index;
    svgRef.current?.setPointerCapture(event.pointerId);
  };

  const onBackgroundDown = (event: React.PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const value = toValue(event.clientX, event.clientY, rect);
    // 只允许在两点之间新增控制点。
    if (value.x <= 0.01 || value.x >= 0.99) return;
    const next = [...points, value].sort((a, b) => a.x - b.x);
    apply(next);
    const index = next.findIndex((point) => point.x === value.x);
    if (index > 0) {
      dragRef.current = index;
      event.currentTarget.setPointerCapture(event.pointerId);
    }
  };

  const onPointerMove = (event: React.PointerEvent<SVGSVGElement>) => {
    if (dragRef.current === null) return;
    const rect = event.currentTarget.getBoundingClientRect();
    movePoint(dragRef.current, toValue(event.clientX, event.clientY, rect));
  };

  const stopDrag = () => {
    dragRef.current = null;
  };

  const onDoubleClickPoint = (index: number) => (event: React.MouseEvent) => {
    event.stopPropagation();
    if (index === 0 || index === points.length - 1) return;
    apply(points.filter((_, current) => current !== index));
  };

  return (
    <div className="curve-editor">
      <div className="curve-tabs">
        {CURVE_CHANNELS.map((item) => (
          <button
            key={item.key}
            type="button"
            className={`curve-tab ${channel === item.key ? "is-active" : ""}`}
            style={{ color: channel === item.key ? item.color : undefined }}
            onClick={() => setChannel(item.key)}
          >
            {item.label}
          </button>
        ))}
        <button
          type="button"
          className="curve-reset"
          disabled={!isDirty}
          title="重置当前通道曲线"
          aria-label="重置当前通道曲线"
          onClick={() => resetCurve(channel)}
        >
          <RotateCcw size={11} />
        </button>
      </div>
      <svg
        ref={svgRef}
        className="curve-canvas"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        onPointerDown={onBackgroundDown}
        onPointerMove={onPointerMove}
        onPointerUp={stopDrag}
        onPointerLeave={stopDrag}
      >
        <rect x={PAD} y={PAD} width={WIDTH - PAD * 2} height={HEIGHT - PAD * 2} className="curve-bg" />
        {[0.25, 0.5, 0.75].map((ratio) => (
          <g key={ratio}>
            <line
              x1={PAD + ratio * (WIDTH - PAD * 2)}
              y1={PAD}
              x2={PAD + ratio * (WIDTH - PAD * 2)}
              y2={HEIGHT - PAD}
              className="curve-grid"
            />
            <line
              x1={PAD}
              y1={PAD + ratio * (HEIGHT - PAD * 2)}
              x2={WIDTH - PAD}
              y2={PAD + ratio * (HEIGHT - PAD * 2)}
              className="curve-grid"
            />
          </g>
        ))}
        <line x1={PAD} y1={HEIGHT - PAD} x2={WIDTH - PAD} y2={PAD} className="curve-diagonal" />
        <path d={path} stroke={activeChannel.color} className="curve-line" />
        {points.map((point, index) => {
          const screen = toScreen(point);
          return (
            <circle
              key={`${index}-${point.x}`}
              cx={screen.x}
              cy={screen.y}
              r={5}
              fill={activeChannel.color}
              className="curve-point"
              onPointerDown={onPointerDown(index)}
              onDoubleClick={onDoubleClickPoint(index)}
            />
          );
        })}
      </svg>
      <p className="param-hint">拖动添加/调整控制点 · 双击删除 · 单击空白处新增</p>
      <p className="curve-hint">
        当前通道：{activeChannel.label}
        {isDirty ? " · 已调整" : " · 未调整"}
      </p>
    </div>
  );
}
