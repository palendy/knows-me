import { AbsoluteFill } from "remotion";
import { theme } from "../theme";
import { ScreenshotShot } from "../components/ScreenshotShot";

// M17 나와 대화 답변. 이 데모의 최고 컷 — 답변 본문을 위→아래로 읽어 내려간다.
export const PersonaScene: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <ScreenshotShot
        src="M17-persona-answer.jpg"
        zoomStart={1.05}
        zoomEnd={1.12}
        panStart={-0.8}
        panEnd={0.7}
      />
    </AbsoluteFill>
  );
};
