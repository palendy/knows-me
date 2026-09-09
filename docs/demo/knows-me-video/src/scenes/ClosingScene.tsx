import { AbsoluteFill, Img, staticFile, interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { theme } from "../theme";
import { Wordmark } from "../components/Wordmark";

// 1:00–1:08 · M01로 복귀 (은은하게 블러) + 워드마크.
export const ClosingScene: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const scale = interpolate(frame, [0, 8 * fps], [1.0, 1.06], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
        <Img
          src={staticFile("shots/M01-dashboard-hero.jpg")}
          style={{
            width: "100%",
            height: "100%",
            objectFit: "cover",
            filter: "blur(16px) brightness(1.06)",
            scale,
            opacity: 0.5,
          }}
        />
      </AbsoluteFill>
      <AbsoluteFill style={{ background: "rgba(250,248,243,0.55)" }} />
      <Wordmark />
    </AbsoluteFill>
  );
};
