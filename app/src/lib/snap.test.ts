import { describe, expect, it } from "vitest";
import { PPQN, snapGridTicks, snapLabel } from "./types";

const FOUR_FOUR: [number, number] = [4, 4];

describe("snapGridTicks", () => {
  it("divides the bar for a straight grid", () => {
    expect(snapGridTicks(FOUR_FOUR, 1, "straight")).toBe(4 * PPQN);
    expect(snapGridTicks(FOUR_FOUR, 4, "straight")).toBe(PPQN);
    expect(snapGridTicks(FOUR_FOUR, 64, "straight")).toBe(PPQN / 16);
  });

  it("makes a dotted step one and a half straight steps", () => {
    expect(snapGridTicks(FOUR_FOUR, 4, "dotted")).toBe(1.5 * PPQN);
    expect(snapGridTicks(FOUR_FOUR, 8, "dotted")).toBe(0.75 * PPQN);
  });

  it("fits three triplet steps in the space of two straight ones", () => {
    expect(snapGridTicks(FOUR_FOUR, 4, "triplet") * 3).toBe(snapGridTicks(FOUR_FOUR, 4, "straight") * 2);
    expect(snapGridTicks(FOUR_FOUR, 8, "triplet")).toBe(PPQN / 3);
  });

  it("splits the bar, not the beat, in odd time signatures", () => {
    // The grid is bar/N, so 1/4 of a 3/4 bar is three quarters of a quarter note.
    expect(snapGridTicks([3, 4], 4, "straight")).toBe(0.75 * PPQN);
    expect(snapGridTicks([7, 8], 1, "straight")).toBe(3.5 * PPQN);
  });

  it("never returns a step below one tick", () => {
    expect(snapGridTicks(FOUR_FOUR, 1e6, "triplet")).toBe(1);
  });
});

describe("snapLabel", () => {
  it("marks dotted and triplet grids", () => {
    expect(snapLabel(4, "straight")).toBe("1/4");
    expect(snapLabel(4, "dotted")).toBe("1/4.");
    expect(snapLabel(64, "triplet")).toBe("1/64T");
  });
});
