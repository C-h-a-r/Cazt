import { formatClock, protocolLabel } from "./api";

describe("formatClock", () => {
  it("formats minutes and seconds", () => {
    expect(formatClock(125)).toBe("2:05");
  });

  it("formats hours", () => {
    expect(formatClock(3723)).toBe("1:02:03");
  });
});

describe("protocolLabel", () => {
  it("names supported protocols", () => {
    expect(protocolLabel("chromecast")).toBe("Chromecast");
    expect(protocolLabel("dlna")).toBe("DLNA");
  });
});
