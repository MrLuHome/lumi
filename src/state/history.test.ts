import { describe, expect, it } from "vitest";
import {
  canRedo,
  canUndo,
  createHistory,
  pushHistory,
  redoHistory,
  undoHistory,
} from "./history";

describe("撤销 / 重做栈", () => {
  it("空栈撤销与重做都返回 null", () => {
    const stack = createHistory<number>();
    expect(canUndo(stack)).toBe(false);
    expect(canRedo(stack)).toBe(false);
    expect(undoHistory(stack, 1)).toBeNull();
    expect(redoHistory(stack, 1)).toBeNull();
  });

  it("撤销回到变更前状态，重做回到最新状态", () => {
    const stack = pushHistory(createHistory<number>(), 0, "曝光");
    const undone = undoHistory(stack, 10);
    expect(undone?.state).toBe(0);
    expect(undone?.label).toBe("曝光");
    expect(canUndo(undone?.history)).toBe(false);
    expect(canRedo(undone?.history)).toBe(true);

    const redone = redoHistory(undone?.history, 0);
    expect(redone?.state).toBe(10);
    expect(canRedo(redone?.history)).toBe(false);
  });

  it("新的变更会清空重做栈", () => {
    const stack = pushHistory(createHistory<number>(), 0, "曝光");
    const undone = undoHistory(stack, 10);
    const next = pushHistory(undone?.history, 0, "对比度");
    expect(next.future).toHaveLength(0);
    expect(next.past).toHaveLength(1);
    expect(next.past[0]?.label).toBe("对比度");
  });

  it("超出上限时丢弃最早记录", () => {
    const limit = 3;
    let stack = createHistory<number>(limit);
    for (let index = 0; index < 6; index += 1) {
      stack = pushHistory(stack, index, `步骤 ${index}`);
    }
    expect(stack.past).toHaveLength(limit);
    expect(stack.past[0]?.state).toBe(3);
    expect(stack.past[limit - 1]?.state).toBe(5);
  });

  it("多步撤销顺序正确", () => {
    let stack = createHistory<string>();
    stack = pushHistory(stack, "a", "第一步");
    stack = pushHistory(stack, "b", "第二步");

    const first = undoHistory(stack, "c");
    expect(first?.state).toBe("b");
    const second = undoHistory(first?.history, "b");
    expect(second?.state).toBe("a");
    expect(canUndo(second?.history)).toBe(false);
  });
});
