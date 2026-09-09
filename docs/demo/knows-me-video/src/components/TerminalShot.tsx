import { AbsoluteFill, interpolate, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { MONO } from "../theme";

// 미확보 컷(M02) 재현: 클로드 세션 트랜스크립트가 흐르는 터미널.
// "사람이 읽기엔 너무 길고 버리기엔 아까운 것" — 스크롤로 그 방대함을 보여준다.
const LINES: { t: string; dim?: boolean; user?: boolean }[] = [
  { t: "> knows-me 데스크톱 앱 창이 안 뜨는데 로그부터 봐줘", user: true },
  { t: "  tauri dev 로그를 확인합니다…", dim: true },
  { t: "  WebView2 초기화 실패 — 3회 재시도 후 복구", dim: true },
  { t: "> 토스 미니앱 실기기 테스트가 매번 다른 데서 막혀", user: true },
  { t: "  Metro 서버 IP 연결 / EADDRINUSE 포트 충돌 / 404…", dim: true },
  { t: "  \"나는 잘 안 되니 네가 직접 해봐\" — 실기기 테스트 위임 패턴", dim: true },
  { t: "> 7일 표본으로는 파라미터 안 건드리는 게 원칙이야", user: true },
  { t: "  MA20/MA60, 봇은 launchd·로그 갱신 시각으로 판단", dim: true },
  { t: "> AI-DLC v2.0으로 AI-네이티브 개발 계속 진행 중", user: true },
  { t: "  오케스트레이터 스킬을 직접 설계·운용합니다", dim: true },
  { t: "> knows-me / 아바타 카드 정의가 아직 안 잡혔어", user: true },
  { t: "  개념을 여러 번 재정의 — 확정 대기", dim: true },
  { t: "> 검증 안 된 걸 완료로 치지 않는 게 내 원칙", user: true },
  { t: "  로컬 실행 환경 16번 중 12번 실패 기록", dim: true },
];

export const TerminalShot: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();

  const enter = interpolate(frame, [0, 0.5 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });

  const lineH = 52;
  const totalScroll = LINES.length * lineH;
  // 짧은 씬 동안 빠르게 위로 스크롤 (방대함을 느끼게)
  const scroll = interpolate(frame, [0.2 * fps, durationInFrames], [0, totalScroll - 480], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.4, 0, 0.4, 1),
  });

  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center", padding: 120 }}>
      <div
        style={{
          width: 1400,
          height: 780,
          borderRadius: 18,
          overflow: "hidden",
          background: "#1C201C",
          boxShadow: "0 24px 70px rgba(30,35,31,0.22)",
          opacity: enter,
          scale: interpolate(enter, [0, 1], [0.985, 1]),
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* title bar */}
        <div
          style={{
            height: 52,
            display: "flex",
            alignItems: "center",
            gap: 9,
            padding: "0 22px",
            background: "#232823",
          }}
        >
          <span style={{ width: 13, height: 13, borderRadius: 99, background: "#E06C5E" }} />
          <span style={{ width: 13, height: 13, borderRadius: 99, background: "#E3B23C" }} />
          <span style={{ width: 13, height: 13, borderRadius: 99, background: "#7DBE6B" }} />
          <span
            style={{
              marginLeft: 16,
              fontFamily: MONO,
              fontSize: 15,
              color: "#828A80",
            }}
          >
            claude — ~/.claude/projects · 431 sessions
          </span>
        </div>
        {/* scrolling transcript */}
        <div style={{ flex: 1, position: "relative", overflow: "hidden", padding: "24px 40px" }}>
          <div style={{ translate: `0px ${-scroll}px` }}>
            {LINES.map((l, i) => (
              <div
                key={i}
                style={{
                  fontFamily: MONO,
                  fontSize: 22,
                  lineHeight: `${lineH}px`,
                  color: l.user ? "#B4D8C3" : l.dim ? "#6F776E" : "#B6BDB3",
                  fontWeight: l.user ? 600 : 400,
                  whiteSpace: "nowrap",
                }}
              >
                {l.t}
              </div>
            ))}
          </div>
          {/* 상·하단 페이드 */}
          <div
            style={{
              position: "absolute",
              inset: 0,
              pointerEvents: "none",
              background:
                "linear-gradient(#1C201C 0%, transparent 12%, transparent 82%, #1C201C 100%)",
            }}
          />
        </div>
      </div>
    </AbsoluteFill>
  );
};
