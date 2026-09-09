import "@testing-library/jest-dom/vitest";

// jsdom has no 2D canvas backend; react-force-graph-2d only renders when one is
// present (GraphView guards on getContext). Stub it to return null so the guard
// takes the "no canvas" path silently instead of jsdom logging "Not implemented".
if (typeof HTMLCanvasElement !== "undefined") {
  HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
}
