import { interpolate, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { FONT, theme } from "../theme";

// 초록 별 로고 마크 (앱 로고의 sparkle 모티프를 간단히 SVG로)
const LogoMark: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 48 48" fill="none">
    <path
      d="M24 3c1.6 8.4 5.6 12.4 14 14-8.4 1.6-12.4 5.6-14 14-1.6-8.4-5.6-12.4-14-14 8.4-1.6 12.4-5.6 14-14Z"
      fill={theme.green}
    />
    <circle cx="37" cy="11" r="3.4" fill={theme.green} opacity={0.6} />
  </svg>
);

// 클로징 워드마크 — 로고 + "Knows Me" + 태그라인.
export const Wordmark: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const appear = interpolate(frame, [0, 0.7 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
  const lift = interpolate(appear, [0, 1], [18, 0]);
  const tagOpacity = interpolate(frame, [0.6 * fps, 1.4 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        alignItems: "center",
        gap: 26,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 20,
          opacity: appear,
          translate: `0px ${lift}px`,
        }}
      >
        <LogoMark size={76} />
        <span
          style={{
            fontFamily: "Georgia, 'Times New Roman', serif",
            fontSize: 78,
            fontWeight: 700,
            color: theme.green,
            letterSpacing: "-0.01em",
          }}
        >
          Knows Me
        </span>
      </div>
      <div
        style={{
          fontFamily: FONT,
          fontSize: 34,
          fontWeight: 500,
          color: theme.ink2,
          opacity: tagOpacity,
          letterSpacing: "-0.01em",
        }}
      >
        작은 흔적이 모여, 나를 이해하는 여정.
      </div>
    </div>
  );
};
