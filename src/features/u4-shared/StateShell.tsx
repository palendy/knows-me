// Renders the loading / empty / error shells so each view can focus on its
// ready state (U4-NFR-U1, BR-E4). Keeping this in one place is also what stops
// the four views from drifting into four different error affordances.

import type { ReactNode } from "react";
import type { ViewState } from "./view-state";
import { muted } from "./styles";

interface Props<T> {
  state: ViewState<T>;
  emptyMessage: string;
  onRetry?: () => void;
  children: (data: T) => ReactNode;
}

export function StateShell<T>({ state, emptyMessage, onRetry, children }: Props<T>) {
  if (state.status === "loading") {
    return <p role="status">불러오는 중…</p>;
  }
  if (state.status === "empty") {
    return (
      <p role="status" style={muted}>
        {emptyMessage}
      </p>
    );
  }
  if (state.status === "error") {
    return (
      <div role="alert">
        <p>{state.message}</p>
        {onRetry && (
          <button type="button" onClick={onRetry}>
            다시 시도
          </button>
        )}
      </div>
    );
  }
  return <>{children(state.data)}</>;
}
