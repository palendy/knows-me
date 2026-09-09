import { AbsoluteFill } from "remotion";
import { theme } from "../theme";
import { TerminalShot } from "../components/TerminalShot";
import { Caption } from "../components/Caption";

// 0:06–0:12 · 세션 트랜스크립트가 흐른다 → "그리고 세션이 끝나면 전부 사라진다."
export const ProblemScene: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <TerminalShot />
      <Caption text="세션이 끝나면 전부 사라진다." from={6} durationInFrames={54} bg={theme.ink} />
    </AbsoluteFill>
  );
};
