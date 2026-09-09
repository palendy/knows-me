// GET /api/connect — Tier3: 심사자가 자기 Claude Code(또는 MCP 클라이언트)에
// 직접 연결하도록 오너별 URL·토큰·`claude mcp add` 한 줄을 돌려준다.
//
// ⚠️ 이 엔드포인트는 의도적으로 토큰을 노출한다(직접 연결에 필요). 30분 스코프
// 데모 전제 — 데모가 끝나면 앱에서 토큰을 폐기(revoke)할 것. 상시 공개 금물.
import { getOwners, mcpEndpoint } from '../lib/owners.mjs';

function slug(name, fallback) {
  const s = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return s || fallback;
}

export default function handler(req, res) {
  const owners = getOwners().map((o) => {
    const url = mcpEndpoint(o);
    const server = `${slug(o.name, o.id)}-knows-me`;
    const command = `claude mcp add --transport http ${server} ${url} --header "Authorization: Bearer ${o.token}"`;
    return { id: o.id, name: o.name, url, token: o.token, command };
  });
  res.status(200).json({ owners });
}
