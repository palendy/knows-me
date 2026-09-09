import { interpolate, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { FONT, theme } from "../theme";

// 좌상단 챕터 라벨 (예: "01 · 수집"). 스크린샷 위에 얹어 맥락을 준다.
export const ChapterLabel: React.FC<{ no: string; title: string }> = ({ no, title }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const opacity = interpolate(frame, [2, 0.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
  const slide = interpolate(opacity, [0, 1], [-10, 0]);

  return (
    <div
      style={{
        position: "absolute",
        top: 56,
        left: 72,
        display: "flex",
        alignItems: "center",
        gap: 12,
        opacity,
        translate: `${slide}px 0px`,
      }}
    >
      <span
        style={{
          fontFamily: FONT,
          fontSize: 20,
          fontWeight: 600,
          color: theme.surface,
          background: theme.lav,
          padding: "6px 14px",
          borderRadius: 999,
          letterSpacing: "0.02em",
        }}
      >
        {no}
      </span>
      <span
        style={{
          fontFamily: FONT,
          fontSize: 24,
          fontWeight: 600,
          color: theme.ink,
          letterSpacing: "-0.01em",
        }}
      >
        {title}
      </span>
    </div>
  );
};
