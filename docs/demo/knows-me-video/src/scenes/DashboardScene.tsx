import { AbsoluteFill, useVideoConfig } from "remotion";
import { theme } from "../theme";
import { ScreenshotShot } from "../components/ScreenshotShot";
import { Caption } from "../components/Caption";
import { ChapterLabel } from "../components/ChapterLabel";

// 0:34–0:41 · M09 대시보드 숫자 세 장. "111개의 기록 — 활동 로그가 아니라, 나에 대해 참인 것."
export const DashboardScene: React.FC = () => {
  const { fps } = useVideoConfig();
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <ScreenshotShot
        src="M09-dashboard-counts.jpg"
        zoomStart={1.04}
        zoomEnd={1.1}
        panStart={-0.7}
        panEnd={0.2}
      />
      <ChapterLabel no="03" title="내 뷰" />
      <Caption text="활동 로그가 아니라, 나에 대해 참인 것." from={Math.round(0.6 * fps)} />
    </AbsoluteFill>
  );
};
