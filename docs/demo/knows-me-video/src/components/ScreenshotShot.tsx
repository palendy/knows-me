import { AbsoluteFill, Img, staticFile, interpolate, useCurrentFrame, useVideoConfig, Easing } from "remotion";
import { theme } from "../theme";

// 스크린샷은 모두 1600x1367. 앱 창(chrome 포함) 통짜 이미지.
const SHOT_W = 1600;
const SHOT_H = 1367;

// 앱 스크린샷을 라운드 프레임에 "통째로" 담아 보여준다.
// 세로가 긴 이미지라 화면 높이에 맞추고(contain), 살짝 Ken Burns 줌.
// zoom: 시작~끝 스케일 배수 (1.0 = 원본). pan: 세로 이동 비율(-1~1, 양수면 아래로).
export const ScreenshotShot: React.FC<{
  src: string; // public/shots 파일명 (확장자 포함)
  zoomStart?: number;
  zoomEnd?: number;
  panStart?: number; // -1(위) ~ 1(아래), 화면에서 잘리는 부분 선택
  panEnd?: number;
}> = ({ src, zoomStart = 1.0, zoomEnd = 1.04, panStart = 0, panEnd = 0 }) => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames, width: cw, height: ch } = useVideoConfig();

  const pad = 70;
  const availH = ch - pad * 2;
  // 이미지를 화면 높이에 맞춤 (세로 fill)
  const baseScale = availH / SHOT_H;

  const t = interpolate(frame, [0, durationInFrames], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.4, 0, 0.2, 1),
  });
  const zoom = interpolate(t, [0, 1], [zoomStart, zoomEnd]);
  const scale = baseScale * zoom;

  const dispW = SHOT_W * scale;
  const dispH = SHOT_H * scale;

  // 세로 팬: 이미지가 프레임(availH)보다 크면 그만큼 이동 가능
  const overflowY = Math.max(0, dispH - availH);
  const pan = interpolate(t, [0, 1], [panStart, panEnd]);
  const translateY = (-pan * overflowY) / 2;

  // punch-in 등장: 짧고 강하게 (살짝 크게 → 제자리)
  const enter = interpolate(frame, [0, 0.35 * fps], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
  const punch = interpolate(enter, [0, 1], [1.06, 1]);

  return (
    <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
      <div
        style={{
          width: dispW,
          maxWidth: cw - pad * 2,
          height: availH,
          borderRadius: 16,
          overflow: "hidden",
          position: "relative",
          boxShadow: "0 24px 70px rgba(30,35,31,0.16), 0 2px 8px rgba(30,35,31,0.06)",
          border: `1px solid ${theme.rule}`,
          background: theme.surface,
          opacity: enter,
          scale: punch,
        }}
      >
        <Img
          src={staticFile(`shots/${src}`)}
          style={{
            position: "absolute",
            width: dispW,
            height: dispH,
            left: 0,
            top: (availH - dispH) / 2 + translateY,
            objectFit: "cover",
          }}
        />
      </div>
    </AbsoluteFill>
  );
};
