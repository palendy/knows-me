import "./index.css";
import { Composition } from "remotion";
import { KnowsMeVideo, TOTAL_DURATION } from "./KnowsMeVideo";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="KnowsMe"
        component={KnowsMeVideo}
        durationInFrames={TOTAL_DURATION}
        fps={30}
        width={1920}
        height={1080}
      />
    </>
  );
};
