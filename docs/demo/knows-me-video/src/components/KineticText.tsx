import { interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { FONT, theme } from "../theme";

// 단어 토큰: 텍스트 + 강조 여부(색 배경 pill)
export type Word = { t: string; hi?: boolean };

// 풀화면 키네틱 타이포. 단어가 순차적으로 아래에서 튀어 오르며 등장,
// 강조 단어에는 컬러 pill 배경. 광고 카피처럼 화면 중앙에 크게.
export const KineticText: React.FC<{
  words: Word[];
  from?: number;
  fontSize?: number;
  hiColor?: string;
  align?: "center" | "left";
  // 씬 끝에서 통째로 사라지는 시점(프레임). 지정 안 하면 유지.
  outAt?: number;
}> = ({ words, from = 0, fontSize = 96, hiColor = theme.lav, align = "center", outAt }) => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();

  const stagger = 4; // 단어 간 등장 간격(프레임)
  const outStart = outAt ?? durationInFrames - Math.round(0.4 * fps);
  const outT = interpolate(frame, [outStart, outStart + Math.round(0.35 * fps)], [0, 1], {
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
        alignItems: align === "center" ? "center" : "flex-start",
        padding: align === "center" ? "0 140px" : "0 130px",
        opacity: interpolate(outT, [0, 1], [1, 0]),
        scale: interpolate(outT, [0, 1], [1, 1.06]),
      }}
    >
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: `${fontSize * 0.16}px ${fontSize * 0.24}px`,
          justifyContent: align === "center" ? "center" : "flex-start",
          maxWidth: 1500,
        }}
      >
        {words.map((w, i) => {
          const wf = frame - from - i * stagger;
          const pop = spring({
            frame: wf,
            fps,
            config: { damping: 13, mass: 0.7, stiffness: 160 },
            durationInFrames: Math.round(0.5 * fps),
          });
          const opacity = interpolate(pop, [0, 0.5], [0, 1], { extrapolateRight: "clamp" });
          const y = interpolate(pop, [0, 1], [fontSize * 0.5, 0]);
          const s = interpolate(pop, [0, 1], [0.6, 1]);

          return (
            <span
              key={i}
              style={{
                fontFamily: FONT,
                fontSize,
                fontWeight: 700,
                lineHeight: 1.18,
                letterSpacing: "-0.02em",
                color: w.hi ? "#FFFFFF" : theme.ink,
                background: w.hi ? hiColor : "transparent",
                padding: w.hi ? "2px 20px" : "2px 0",
                borderRadius: 16,
                opacity,
                translate: `0px ${y}px`,
                scale: s,
                display: "inline-block",
                boxShadow: w.hi ? "0 8px 26px rgba(30,35,31,0.2)" : "none",
              }}
            >
              {w.t}
            </span>
          );
        })}
      </div>
    </div>
  );
};
