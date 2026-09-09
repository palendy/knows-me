// 브라우저 클라이언트. 오너 선택 → 자연어 질문 → /api/chat(서버측 에이전트가
// 우리 MCP 4툴 호출) → 답변 + 도구 호출 트레이스. 모든 서버 텍스트는 textContent로
// 렌더(주입 방지). 서버리스 프록시가 터널 URL·토큰을 서버측에서만 보유한다.

const $ = (s, r = document) => r.querySelector(s);
const el = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text != null) n.textContent = text;
  return n;
};

const chatEl = $('#chat');
const formEl = $('#form');
const inputEl = $('#input');
const sendEl = $('#send');
const ownersEl = $('#owners');

let owners = [];
let currentOwner = null;
let history = []; // [{role:'user'|'assistant', content:string}]
let busy = false;

const SUGGESTIONS = [
  '어떤 범주를 물어볼 수 있어?',
  '네가 아는 내용 간단히 소개해줘',
  '배포 규칙 있어?',
];

// ---- init ----
init();

async function init() {
  renderSuggestions();
  try {
    const r = await fetch('/api/owners');
    owners = (await r.json()).owners || [];
  } catch {
    owners = [];
  }
  if (!owners.length) {
    renderEmpty('오너가 아직 설정되지 않았습니다. (Vercel env에 OWNER1_URL/TOKEN 등을 넣고 배포하세요.)');
    ownersEl.append(el('span', 'label', '설정된 오너 없음'));
    inputEl.disabled = sendEl.disabled = true;
    return;
  }
  renderOwners();
  selectOwner(owners[0].id);
  loadConnect();
}

// ---- owners ----
function renderOwners() {
  ownersEl.replaceChildren();
  for (const o of owners) {
    const b = el('button', 'owner-btn', o.name);
    b.type = 'button';
    b.setAttribute('role', 'tab');
    b.dataset.id = o.id;
    b.addEventListener('click', () => selectOwner(o.id));
    ownersEl.append(b);
  }
}

function selectOwner(id) {
  if (currentOwner === id) return;
  currentOwner = id;
  history = [];
  for (const b of ownersEl.children) {
    b.setAttribute('aria-selected', String(b.dataset.id === id));
  }
  const name = owners.find((o) => o.id === id)?.name || '';
  renderEmpty(`“${name}”에게 물어보세요. 질문하면 실시간으로 이 팀원의 실행 중인 앱에 MCP로 연결합니다.`);
}

// ---- suggestions ----
function renderSuggestions() {
  const box = $('#suggestions');
  box.replaceChildren();
  for (const s of SUGGESTIONS) {
    const c = el('button', 'chip', s);
    c.type = 'button';
    c.addEventListener('click', () => {
      inputEl.value = s;
      inputEl.focus();
    });
    box.append(c);
  }
}

// ---- chat rendering ----
function renderEmpty(text) {
  chatEl.replaceChildren(el('div', 'empty', text));
}

function addBubble(role, text, { error = false } = {}) {
  if (chatEl.querySelector('.empty')) chatEl.replaceChildren();
  const row = el('div', `msg ${role}`);
  const bubble = el('div', `bubble${error ? ' err' : ''}`, text);
  row.append(bubble);
  chatEl.append(row);
  chatEl.scrollTop = chatEl.scrollHeight;
  return bubble;
}

function addTrace(bubble, trace) {
  if (!trace || !trace.length) return;
  const d = el('details', 'trace');
  d.append(el('summary', null, `🔧 MCP 도구 호출 ${trace.length}회 (실제 질의 내역)`));
  for (const t of trace) {
    const item = el('div', 'trace-item');
    item.append(el('div', 'trace-call', `${t.name}(${JSON.stringify(t.input)})`));
    const out = el('div', `trace-out${t.isError ? ' err' : ''}`, t.output);
    item.append(out);
    d.append(item);
  }
  bubble.append(d);
  chatEl.scrollTop = chatEl.scrollHeight;
}

// ---- send ----
formEl.addEventListener('submit', async (e) => {
  e.preventDefault();
  const text = inputEl.value.trim();
  if (!text || busy || !currentOwner) return;

  addBubble('user', text);
  history.push({ role: 'user', content: text });
  inputEl.value = '';
  setBusy(true);

  const typing = addBubble('assistant', '생각 중… (MCP 질의)', {});
  typing.classList.add('typing');

  try {
    const r = await fetch('/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: currentOwner, messages: history }),
    });
    const data = await r.json();
    typing.closest('.msg').remove();

    if (data.error) {
      addBubble('assistant', data.error, { error: true });
    } else {
      const bubble = addBubble('assistant', data.answer || '(응답 없음)');
      addTrace(bubble, data.trace);
      history.push({ role: 'assistant', content: data.answer || '' });
    }
  } catch (err) {
    typing.closest('.msg')?.remove();
    addBubble('assistant', `요청 실패: ${err?.message || err}`, { error: true });
  } finally {
    setBusy(false);
    inputEl.focus();
  }
});

function setBusy(b) {
  busy = b;
  sendEl.disabled = b;
  inputEl.disabled = b;
}

// ---- tier 3: 직접 연결 ----
async function loadConnect() {
  const list = $('#connect-list');
  try {
    const r = await fetch('/api/connect');
    const items = (await r.json()).owners || [];
    list.replaceChildren();
    for (const o of items) {
      const row = el('div', 'connect-row');
      row.append(el('h3', null, o.name));
      const cmd = el('div', 'cmd');
      cmd.append(el('code', null, o.command));
      const copy = el('button', 'copy', '복사');
      copy.type = 'button';
      copy.addEventListener('click', async () => {
        try {
          await navigator.clipboard.writeText(o.command);
          copy.textContent = '복사됨';
          setTimeout(() => (copy.textContent = '복사'), 1500);
        } catch {
          copy.textContent = '실패';
        }
      });
      cmd.append(copy);
      row.append(cmd);
      list.append(row);
    }
  } catch {
    list.replaceChildren(el('p', 'connect-note', '연결 정보를 불러오지 못했습니다.'));
  }
}
