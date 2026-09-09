import { AbsoluteFill } from "remotion";
import { theme } from "../theme";
import { ScreenshotShot } from "../components/ScreenshotShot";
import { ChapterLabel } from "../components/ChapterLabel";

// M03b 연결 소스 카탈로그(고해상도). 카피는 앞선 타이포 컷이 담당.
export const CollectScene: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <ScreenshotShot
        src="M03b-connect.jpg"
        zoomStart={1.06}
        zoomEnd={1.12}
        panStart={-0.3}
        panEnd={0.4}
      />
      <ChapterLabel no="01" title="수집" />
    </AbsoluteFill>
  );
};
