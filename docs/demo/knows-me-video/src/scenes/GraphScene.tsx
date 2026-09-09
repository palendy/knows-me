import { AbsoluteFill } from "remotion";
import { theme } from "../theme";
import { ScreenshotShot } from "../components/ScreenshotShot";
import { ChapterLabel } from "../components/ChapterLabel";

// M11 지식 그래프. 전체 → 노드 뭉치로 줌인. 카피는 앞선 타이포 컷이 담당.
export const GraphScene: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <ScreenshotShot
        src="M11-graph.jpg"
        zoomStart={1.05}
        zoomEnd={1.14}
        panStart={0}
        panEnd={0.5}
      />
      <ChapterLabel no="03" title="내 뷰" />
    </AbsoluteFill>
  );
};
