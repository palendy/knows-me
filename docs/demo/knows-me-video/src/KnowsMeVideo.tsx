import { AbsoluteFill } from "remotion";
import {
  TransitionSeries,
  linearTiming,
  springTiming,
  TransitionPresentation,
} from "@remotion/transitions";
import { fade } from "@remotion/transitions/fade";
import { slide } from "@remotion/transitions/slide";
import { wipe } from "@remotion/transitions/wipe";
import "./fonts";
import { theme } from "./theme";

import { OpeningScene } from "./scenes/OpeningScene";
import { ProblemScene } from "./scenes/ProblemScene";
import { CollectScene } from "./scenes/CollectScene";
import { QueueScene } from "./scenes/QueueScene";
import { GraphScene } from "./scenes/GraphScene";
import { PersonaScene } from "./scenes/PersonaScene";
import { ClosingScene } from "./scenes/ClosingScene";
import { TypoScene } from "./scenes/TypoScene";

// ── 광고 리듬: 타이포 훅 → 앱 화면, 전환을 매번 다르게 ──
// 타이포 컷은 짧고(≈1.5s) 임팩트, 앱 컷은 조금 길게(≈2–2.6s).

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyPresentation = TransitionPresentation<any>;
type Trans = { presentation: AnyPresentation; d: number; timing: "spring" | "linear" };
const whip = (dir: "from-left" | "from-right"): Trans => ({
  presentation: slide({ direction: dir }),
  d: 10,
  timing: "spring",
});
const push = (dir: "from-bottom" | "from-top"): Trans => ({
  presentation: slide({ direction: dir }),
  d: 12,
  timing: "spring",
});
const wipeT = (dir: "from-left" | "from-top"): Trans => ({
  presentation: wipe({ direction: dir }),
  d: 12,
  timing: "linear",
});
const fadeT = (): Trans => ({ presentation: fade(), d: 10, timing: "linear" });

// 씬 + 그 씬으로 "들어가는" 전환(첫 씬은 전환 없음)
type Item = { d: number; C: React.FC; name: string; trans?: Trans };

export const SCENES: Item[] = [
  { d: 42, C: () => <TypoScene words={[{ t: "나는 매일" }, { t: "AI에" }, { t: "내 맥락을", hi: true }, { t: "쏟아붓는다" }]} />, name: "Hook" },
  { d: 78, C: OpeningScene, name: "Hero", trans: fadeT() },
  { d: 54, C: ProblemScene, name: "Problem", trans: whip("from-right") },
  { d: 38, C: () => <TypoScene words={[{ t: "기록은" }, { t: "이미 있다", hi: true }, { t: "—" }, { t: "아무도" }, { t: "안 읽었을 뿐" }]} />, name: "TypoCollect", trans: wipeT("from-left") },
  { d: 66, C: CollectScene, name: "Collect", trans: push("from-bottom") },
  { d: 40, C: () => <TypoScene words={[{ t: "확실하지 않으면" }, { t: "저장하지 않는다", hi: true }]} hiColor={theme.amber} glow="rgba(168,103,46,0.10)" />, name: "TypoQueue", trans: whip("from-left") },
  { d: 70, C: QueueScene, name: "Queue", trans: push("from-bottom") },
  { d: 38, C: () => <TypoScene words={[{ t: "그리고" }, { t: "서로", hi: true }, { t: "얽혀 있다", hi: true }]} hiColor={theme.green} glow="rgba(54,93,75,0.10)" />, name: "TypoGraph", trans: whip("from-right") },
  { d: 66, C: GraphScene, name: "Graph", trans: push("from-top") },
  { d: 46, C: () => <TypoScene words={[{ t: "“내가 요즘" }, { t: "제일 걱정하는", hi: true }, { t: "게 뭘까?”" }]} fontSize={86} />, name: "TypoPersona", trans: wipeT("from-top") },
  { d: 108, C: PersonaScene, name: "Persona", trans: push("from-bottom") },
  { d: 36, C: () => <TypoScene words={[{ t: "검색으로는" }, { t: "못 만드는 답", hi: true }]} hiColor={theme.green} glow="rgba(54,93,75,0.10)" />, name: "TypoPunch", trans: fadeT() },
  { d: 78, C: ClosingScene, name: "Closing", trans: fadeT() },
];

const timingFor = (t: Trans) =>
  t.timing === "spring"
    ? springTiming({ config: { damping: 200 }, durationInFrames: t.d })
    : linearTiming({ durationInFrames: t.d });

export const TOTAL_DURATION =
  SCENES.reduce((s, x) => s + x.d, 0) -
  SCENES.reduce((s, x) => s + (x.trans ? x.trans.d : 0), 0);

export const KnowsMeVideo: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.ground }}>
      <TransitionSeries>
        {SCENES.flatMap((scene, i) => {
          const Comp = scene.C;
          const els: React.ReactNode[] = [];
          if (scene.trans) {
            els.push(
              <TransitionSeries.Transition
                key={`t-${i}`}
                presentation={scene.trans.presentation}
                timing={timingFor(scene.trans)}
              />,
            );
          }
          els.push(
            <TransitionSeries.Sequence key={`seq-${i}`} durationInFrames={scene.d} name={scene.name}>
              <Comp />
            </TransitionSeries.Sequence>,
          );
          return els;
        })}
      </TransitionSeries>
    </AbsoluteFill>
  );
};
