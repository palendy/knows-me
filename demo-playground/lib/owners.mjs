// 서버측에서만 실행됨(서버리스 함수). 터널 URL·토큰은 여기서만 읽히고,
// 클라이언트로는 절대 나가지 않는다(공개 목록은 id/name만).
//
// env 규약 (오너마다 3개, i = 1..6):
//   OWNER{i}_NAME   표시 이름 (예: "지빈")           — 공개 가능
//   OWNER{i}_URL    터널 주소 (예: https://xx.trycloudflare.com)
//   OWNER{i}_TOKEN  컨슈머 Bearer 토큰               — 비밀
//
// URL 뒤에 /mcp 는 있어도 없어도 됨(프록시가 붙여줌).

const MAX_OWNERS = 6;

/** env에 설정된 오너들(비밀 포함). 서버 내부 전용. */
export function getOwners() {
  const owners = [];
  for (let i = 1; i <= MAX_OWNERS; i++) {
    const url = process.env[`OWNER${i}_URL`];
    const token = process.env[`OWNER${i}_TOKEN`];
    if (!url || !token) continue;
    owners.push({
      id: `owner${i}`,
      name: process.env[`OWNER${i}_NAME`]?.trim() || `Owner ${i}`,
      url: url.trim().replace(/\/+$/, ''),
      token: token.trim(),
    });
  }
  return owners;
}

/** id로 오너 하나(비밀 포함). 없으면 undefined. */
export function findOwner(id) {
  return getOwners().find((o) => o.id === id);
}

/** 오너의 MCP 엔드포인트(/mcp 보장). */
export function mcpEndpoint(owner) {
  return owner.url.endsWith('/mcp') ? owner.url : `${owner.url}/mcp`;
}
