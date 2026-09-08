// The four states every U4 view renders explicitly (U4-NFR-U1, BR-E4).
//
// Making "empty" a state of its own — rather than a special case of "ready" —
// is what forces each view to say what the owner should do next instead of
// showing a blank panel.

export type ViewState<T> =
  | { status: "loading" }
  | { status: "ready"; data: T }
  | { status: "empty" }
  | { status: "error"; message: string };

export const loading = <T,>(): ViewState<T> => ({ status: "loading" });
export const ready = <T,>(data: T): ViewState<T> => ({ status: "ready", data });
export const empty = <T,>(): ViewState<T> => ({ status: "empty" });
export const failed = <T,>(message: string): ViewState<T> => ({
  status: "error",
  message,
});

/** Turn an unknown thrown value into a message safe to show the owner. */
export function messageOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return "알 수 없는 오류가 발생했습니다";
}

/**
 * Run a fetch and classify the result, treating "no data" as `empty`.
 *
 * `isEmpty` is passed in because emptiness is per-view: a dashboard with all
 * counts at zero is empty, a graph with no nodes is empty, and neither rule
 * generalizes.
 */
export async function load<T>(
  fetch: () => Promise<T>,
  isEmpty: (data: T) => boolean,
): Promise<ViewState<T>> {
  try {
    const data = await fetch();
    return isEmpty(data) ? empty<T>() : ready(data);
  } catch (err) {
    return failed<T>(messageOf(err));
  }
}
