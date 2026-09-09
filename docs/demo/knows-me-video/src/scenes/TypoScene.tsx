import { AbsoluteFill, interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { theme } from "../theme";
import { KineticText, Word } from "../components/KineticText";

// 풀화면 카피 컷. 크림 배경 위 키네틱 타이포. 광고 리듬의 "훅".
// 배경에 은은한 라디얼 글로우로 밋밋함 방지.
export const TypoScene: React.FC<{
  words: Word[];
  hiColor?: string;
  fontSize?: number;
  glow?: string;
}> = ({ words, hiColor = theme.lav, fontSize = 100, glow = "rgba(110,95,163,0.10)" }) => {
  const frame = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();

  // 배경 글로우가 아주 천천히 확장
  const g = interpolate(frame, [0, durationInFrames], [0.9, 1.15], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <AbsoluteFill style={{ background: theme.ground, overflow: "hidden" }}>
      <AbsoluteFill
        style={{
          background: `radial-gradient(circle at 50% 46%, ${glow}, transparent 60%)`,
          scale: g,
        }}
      />
      <KineticText words={words} fontSize={fontSize} hiColor={hiColor} />
    </AbsoluteFill>
  );
};
