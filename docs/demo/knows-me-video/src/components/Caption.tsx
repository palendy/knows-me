import { interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { FONT, theme } from "../theme";

// 광고형 자막: 글자에만 배경이 붙는 pill, 팝 인 + 빠른 팝 아웃.
// - 흰 배경 박스 없음(화면을 가리지 않음)
// - 등장: 아래에서 튀어오르며 스케일 pop
// - 퇴장: 짧고 또렷하게 사라짐
export const Caption: React.FC<{
  text: string;
  from?: number; // 씬 내부 프레임 기준 등장 시작
  durationInFrames?: number; // 유지 길이
  // 강조 색상 (기본 보라). 대비 위해 흰 글자.
  bg?: string;
}> = ({ text, from = 0, durationInFrames, bg = theme.lav }) => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames: total } = useVideoConfig();
  const life = durationInFrames ?? total - from;
  const local = frame - from;
  const end = from + life;

  // 팝 인 (spring)
  const pop = spring({
    frame: local,
    fps,
    config: { damping: 14, mass: 0.6, stiffness: 170 },
    durationInFrames: Math.round(0.5 * fps),
  });
  // 팝 아웃 (마지막 0.28초, 빠르게 위로 튀며 사라짐)
  const outStart = end - Math.round(0.28 * fps);
  const outT = interpolate(frame, [outStart, end], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const scale = interpolate(pop, [0, 1], [0.7, 1]) * interpolate(outT, [0, 1], [1, 1.12]);
  const opacity =
    interpolate(pop, [0, 0.6], [0, 1], { extrapolateRight: "clamp" }) *
    interpolate(outT, [0, 1], [1, 0]);
  const lift = interpolate(pop, [0, 1], [26, 0]) - outT * 24;

  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        bottom: 108,
        display: "flex",
        justifyContent: "center",
        padding: "0 100px",
        opacity,
        translate: `0px ${lift}px`,
        scale,
      }}
    >
      {/* 글자에만 배경 (inline pill) */}
      <span
        style={{
          fontFamily: FONT,
          fontSize: 46,
          fontWeight: 700,
          lineHeight: 1.35,
          letterSpacing: "-0.015em",
          color: "#FFFFFF",
          background: bg,
          padding: "10px 26px",
          borderRadius: 14,
          textAlign: "center",
          boxDecorationBreak: "clone",
          WebkitBoxDecorationBreak: "clone",
          boxShadow: "0 8px 26px rgba(30,35,31,0.22)",
        }}
      >
        {text}
      </span>
    </div>
  );
};
