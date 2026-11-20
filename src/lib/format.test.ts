import { describe, expect, it } from "vitest";
import { formatDuration, formatFileSize, formatMegapixels, formatZoom } from "./format";

describe("格式化", () => {
  it("文件大小按单位换算", () => {
    expect(formatFileSize(0)).toBe("—");
    expect(formatFileSize(512)).toBe("512 B");
    expect(formatFileSize(2048)).toBe("2.0 KB");
    expect(formatFileSize(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatFileSize(120 * 1024 * 1024)).toBe("120 MB");
  });

  it("缩放比例显示", () => {
    expect(formatZoom(1)).toBe("100%");
    expect(formatZoom(0.157)).toBe("16%");
    expect(formatZoom(0.045)).toBe("4.5%");
    expect(formatZoom(2.5)).toBe("250%");
  });

  it("像素量换算", () => {
    expect(formatMegapixels(6000, 4000)).toBe("24.0 MP");
    expect(formatMegapixels(1200, 800)).toBe("0.96 MP");
  });

  it("耗时显示", () => {
    expect(formatDuration(38)).toBe("38 ms");
    expect(formatDuration(1250)).toBe("1.25 s");
  });
});
