import { loadFont as loadSansKR } from "@remotion/google-fonts/IBMPlexSansKR";
import { loadFont as loadMono } from "@remotion/google-fonts/IBMPlexMono";

// 한글 서브셋 포함해서 로드. weights는 앱에서 쓰는 300~700 범위.
export const sansKR = loadSansKR("normal", {
  weights: ["300", "400", "500", "600", "700"],
  subsets: ["korean", "latin"],
});

export const mono = loadMono("normal", {
  weights: ["400", "500", "600"],
  subsets: ["latin"],
});
