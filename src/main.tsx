import React from "react";
import ReactDOM from "react-dom/client";
import { OverlayScrollbars } from "overlayscrollbars";
import "overlayscrollbars/styles/overlayscrollbars.css";
import { App } from "./App";
import "pretendard/dist/web/variable/pretendardvariable.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// Overlay scrollbars for the whole page — a thumb that floats over the content,
// auto-hides, and reserves no layout width (so nothing shifts). Replaces the
// native bar. Initialized on <body>, which OverlayScrollbars handles as the
// document scroller.
OverlayScrollbars(document.body, {
  scrollbars: { autoHide: "leave", autoHideDelay: 700 },
});
