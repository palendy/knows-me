// knows-me 앱 톤에서 뽑은 디자인 토큰. demo-plan.html + 실제 스크린샷 기준.
export const theme = {
  ground: "#FAF8F3", // 크림 배경
  surface: "#FFFFFF",
  surface2: "#F3F0E8",
  ink: "#1E231F", // 진한 먹색 제목
  ink2: "#4C544C",
  muted: "#878D84",
  rule: "#E3DFD3",

  // 앱의 보라 액센트 (사이드바 active, 버튼)
  lav: "#6E5FA3",
  lavSoft: "#ECE9F6",
  lavInk: "#4A3F73",

  // 초록 (로고, "확정 사실")
  green: "#365D4B",
  greenSoft: "#E5EDE8",

  // 살구/따뜻한 톤 ("대기 중인 질문" 카드)
  amber: "#A8672E",
  amberSoft: "#F6EADF",

  flag: "#A8672E",
} as const;

export const FONT = "'IBM Plex Sans KR', -apple-system, BlinkMacSystemFont, 'Apple SD Gothic Neo', sans-serif";
export const MONO = "'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, monospace";
