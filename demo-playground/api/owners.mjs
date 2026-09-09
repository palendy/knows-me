// GET /api/owners — 오너 선택 UI용 공개 목록(id/name만, 비밀 제외).
import { getOwners } from '../lib/owners.mjs';

export default function handler(req, res) {
  const owners = getOwners().map((o) => ({ id: o.id, name: o.name }));
  res.status(200).json({ owners });
}
