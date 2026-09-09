import { AbsoluteFill } from "remotion";
import { theme } from "../theme";
import { ScreenshotShot } from "../components/ScreenshotShot";
import { ChapterLabel } from "../components/ChapterLabel";

// M13 인터뷰 대기열. 카피는 앞선 타이포 컷이 담당.
export const QueueScene: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <ScreenshotShot
        src="M13-queue.jpg"
        zoomStart={1.05}
        zoomEnd={1.1}
        panStart={-0.5}
        panEnd={0.3}
      />
      <ChapterLabel no="02" title="대기열" />
    </AbsoluteFill>
  );
};
