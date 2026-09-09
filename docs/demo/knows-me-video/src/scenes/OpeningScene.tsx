import { AbsoluteFill, staticFile, interpolate, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { Video } from "@remotion/media";
import { theme } from "../theme";

// knows-me 히어로 애니메이션 영상. 브랜드 첫인상 컷. (오디오 음소거)
export const OpeningScene: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  // 부드러운 punch-in (살짝 크게 → 제자리)
  const scale = interpolate(frame, [0, 1 * fps], [1.05, 1.0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
  // 짧은 인트로 밝기 상승
  const brightness = interpolate(frame, [0, 0.4 * fps], [0.9, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
        <Video
          src={staticFile("hero.mp4")}
          muted
          style={{
            width: "100%",
            height: "100%",
            objectFit: "cover",
            filter: `brightness(${brightness})`,
            scale,
          }}
        />
      </AbsoluteFill>
    </AbsoluteFill>
  );
};
