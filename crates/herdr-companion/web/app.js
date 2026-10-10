'use strict';

// Everything the server returns that came from an agent or a terminal is
// untrusted text: it is only ever placed with textContent, never as markup.

const TOKEN_KEY = 'herdr-companion-token';
const KINDS = ['claude', 'codex', 'gemini', 'opencode', 'pi', 'amp', 'copilot', 'cursor', 'cline', 'droid', 'kimi', 'qwen'];
const KEYS = [
  ['↑', 'up'], ['↓', 'down'], ['←', 'left'], ['→', 'right'], ['Enter', 'enter'],
  ['Esc', 'esc'], ['Tab', 'tab'], ['Space', 'space'], ['⌫', 'backspace'], ['^C', 'ctrl+c'],
  ['1', '1'], ['2', '2'], ['3', '3'], ['y', 'y'], ['n', 'n'],
];
const STATUS_ORDER = { blocked: 0, working: 1, done: 2, idle: 3, unknown: 4 };

const state = {
  token: readToken(),
  tab: 'inbox',
  cursor: 0,
  requests: [],
  agents: [],
  sessions: [],
  herdrError: null,
  events: [],
  activity: new Map(), // pane_id → what it is doing right now
  panel: null, // { paneId, sessionId, title }
  panelView: 'chat',
  online: false,
};

const $ = (id) => document.getElementById(id);

function h(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
    else if (value === true) node.setAttribute(key, '');
    else node.setAttribute(key, String(value));
  }
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return node;
}

function readToken() {
  try { return localStorage.getItem(TOKEN_KEY) || ''; } catch { return ''; }
}

function writeToken(token) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch { /* Private mode: the token lasts for this visit only. */ }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const basename = (path) => (path || '').split('/').filter(Boolean).pop() || '';
// "api · api · idle" reads as a glitch; repeated parts are shown once.
const joinParts = (parts) => [...new Set(parts.filter(Boolean))].join(' · ');

class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function api(path, { method = 'GET', body } = {}) {
  const response = await fetch(path, {
    method,
    headers: {
      Authorization: `Bearer ${state.token}`,
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: 'no-store',
  });
  const text = await response.text();
  const data = text ? JSON.parse(text) : null;
  if (response.status === 401) {
    showSetup('The token was refused.');
    throw new ApiError(401, 'unauthorized');
  }
  if (!response.ok) throw new ApiError(response.status, (data && data.error) || `HTTP ${response.status}`);
  return data;
}

function debounce(fn, ms) {
  let timer = null;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

function toast(message) {
  const banner = $('banner');
  banner.textContent = message;
  banner.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { renderBanner(); }, 4000);
}

/* ---------- Setup ---------- */

function showSetup(error = '') {
  $('app').hidden = true;
  $('agent').hidden = true;
  $('setup').hidden = false;
  $('setup-error').textContent = error;
}

// The pairing QR code opens /#token=…. Browsers never send the fragment to
// the server. An installed iOS app keeps storage apart from Safari, so the
// link stays in Safari's address bar for Add to Home Screen to carry over,
// and is cleared once the token lands in the installed app's storage.
function tokenFromLink() {
  const token = new URLSearchParams(location.hash.slice(1)).get('token');
  if (!token) return '';
  const installed = window.matchMedia('(display-mode: standalone)').matches || navigator.standalone === true;
  if (installed) history.replaceState(null, '', location.pathname + location.search);
  return token;
}

async function connect(token) {
  state.token = token;
  try {
    await api('/v1/requests');
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 401) showSetup(`Could not reach the companion: ${error.message}`);
    return false;
  }
  writeToken(token);
  setOnline(true);
  $('setup').hidden = true;
  $('app').hidden = false;
  return true;
}

/* ---------- Data ---------- */

async function refreshRequests() {
  const data = await api('/v1/requests');
  state.requests = data.requests;
  renderInbox();
  renderAgents();
}

async function refreshAgents() {
  try {
    const data = await api('/v1/agents');
    state.agents = data.agents;
    state.herdrError = null;
  } catch (error) {
    if (error.status === 401) return;
    state.agents = [];
    state.herdrError = error.message;
    const sessions = await api('/v1/sessions').catch(() => ({ sessions: [] }));
    state.sessions = sessions.sessions;
  }
  renderAgents();
  renderInbox();
  renderBanner();
  renderPanelHeader();
}

const refreshRequestsSoon = debounce(() => refreshRequests().catch(() => {}), 150);
const refreshAgentsSoon = debounce(() => refreshAgents(), 400);
const refreshChatSoon = debounce(() => refreshChat(), 500);

function agentFor(paneId) {
  return state.agents.find((agent) => agent.pane_id === paneId);
}

function agentLabel(agent) {
  return agent.name || agent.display_agent || agent.agent || agent.pane_id;
}

function originLabel(item) {
  const agent = item.pane_id ? agentFor(item.pane_id) : null;
  if (agent) return joinParts([agentLabel(agent), agent.workspace_label || basename(agent.cwd)]);
  return basename(item.cwd) || item.session_id.slice(0, 8);
}

/* ---------- Event feed ---------- */

function describe(event) {
  switch (event.kind) {
    case 'permission_requested': return `Needs approval: ${event.tool_name} ${event.summary || ''}`;
    case 'permission_resolved': return `Prompt ${event.request_id}: ${event.outcome.replace('_', ' ')}`;
    case 'notification': return event.message || event.notification_type || 'Notification';
    case 'stopped': return `Finished${event.last_assistant_message ? `: ${event.last_assistant_message.slice(0, 160)}` : ''}`;
    case 'prompt_submitted': return `Prompt: ${event.prompt.slice(0, 160)}`;
    case 'tool_started': return `${event.tool_name} ${event.summary}`;
    case 'tool_finished': return `${event.failed ? 'Failed' : 'Done'}: ${event.tool_name} ${event.summary}`;
    default: return event.hook_event_name || event.kind;
  }
}

function trackActivity(event) {
  const pane = event.pane_id;
  if (!pane) return;
  if (event.kind === 'tool_started') state.activity.set(pane, `${event.tool_name} ${event.summary}`.trim());
  if (event.kind === 'stopped' || event.kind === 'permission_requested') state.activity.delete(pane);
}

function handleEvents(events) {
  let chatTouched = false;
  for (const event of events) {
    state.events.push(event);
    trackActivity(event);
    const pane = event.pane_id;
    if (event.kind.startsWith('permission_')) refreshRequestsSoon();
    if (['stopped', 'prompt_submitted', 'permission_requested', 'permission_resolved'].includes(event.kind)) refreshAgentsSoon();
    const panel = state.panel;
    if (panel && ((pane && pane === panel.paneId) || event.session_id === panel.sessionId)) {
      if (!panel.sessionId && event.session_id) panel.sessionId = event.session_id;
      chatTouched = true;
    }
  }
  state.events.splice(0, Math.max(0, state.events.length - 300));
  renderActivity();
  renderAgents();
  renderPanelActivity();
  if (chatTouched) refreshChatSoon();
}

async function pollLoop() {
  for (;;) {
    if (!state.token || !$('setup').hidden) {
      await sleep(1000);
      continue;
    }
    try {
      const page = await api(`/v1/events?after=${state.cursor}&wait=25`);
      if (page.lost) refreshRequestsSoon();
      state.cursor = page.next;
      setOnline(true);
      if (page.events.length) handleEvents(page.events);
    } catch {
      setOnline(false);
      await sleep(3000);
    }
  }
}

function setOnline(online) {
  if (state.online === online) return;
  state.online = online;
  $('conn').classList.toggle('online', online);
  $('conn').title = online ? 'Connected' : 'Reconnecting…';
}

/* ---------- Inbox ---------- */

const cards = new Map(); // request id → card element, so typing survives refreshes

function renderInbox() {
  const root = $('tab-inbox');
  const ids = new Set(state.requests.map((request) => request.id));
  for (const [id, card] of cards) {
    if (!ids.has(id)) {
      card.remove();
      cards.delete(id);
    }
  }
  for (const request of state.requests) {
    if (!cards.has(request.id)) {
      const card = requestCard(request);
      cards.set(request.id, card);
      root.append(card);
    }
  }
  // Agent names may arrive after the cards; labels follow without
  // rebuilding cards someone is typing in.
  for (const request of state.requests) {
    const origin = cards.get(request.id)?.querySelector('.origin');
    if (origin) origin.textContent = originLabel(request);
  }
  root.querySelector('.empty')?.remove();
  if (!state.requests.length) root.append(h('p', { class: 'empty', text: 'Nothing is waiting for you.' }));
  const count = state.requests.length;
  $('inbox-count').hidden = !count;
  $('inbox-count').textContent = String(count);
  if (navigator.setAppBadge) (count ? navigator.setAppBadge(count) : navigator.clearAppBadge()).catch(() => {});
}

async function decide(request, decision, card) {
  for (const button of card.querySelectorAll('button')) button.disabled = true;
  try {
    await api(`/v1/requests/${request.id}`, { method: 'POST', body: decision });
  } catch (error) {
    toast(error.status === 404 ? 'That prompt already closed.' : `Could not answer: ${error.message}`);
  }
  await refreshRequests().catch(() => {});
}

function requestCard(request) {
  const card = h('article', { class: 'card' });
  card.append(h('div', { class: 'card-head' },
    h('strong', { class: 'origin', text: originLabel(request) }),
    h('span', { class: 'chip', text: request.tool_name })));
  if (request.tool_name === 'AskUserQuestion') {
    card.append(questionForm(request, card));
    return card;
  }
  card.append(...toolDetails(request));
  const reason = h('textarea', { rows: 2, placeholder: 'Tell the agent why, or what to do instead', hidden: true });
  const edit = request.tool_name === 'Bash'
    ? h('textarea', { rows: 3, class: 'code', hidden: true })
    : null;
  if (edit) edit.value = request.tool_input.command || '';
  card.append(reason);
  if (edit) card.append(edit);
  const allow = h('button', {
    class: 'primary',
    text: 'Allow',
    onclick: () => {
      if (edit && !edit.hidden) {
        decide(request, { behavior: 'allow', updated_input: { ...request.tool_input, command: edit.value } }, card);
      } else {
        decide(request, { behavior: 'allow' }, card);
      }
    },
  });
  const deny = h('button', {
    class: 'danger',
    text: 'Deny',
    onclick: () => {
      if (reason.hidden) {
        reason.hidden = false;
        reason.focus();
        deny.textContent = 'Send denial';
        return;
      }
      if (!reason.value.trim()) {
        reason.focus();
        return;
      }
      decide(request, { behavior: 'deny', message: reason.value.trim() }, card);
    },
  });
  const actions = h('div', { class: 'actions' }, allow, deny);
  if (edit) {
    actions.append(h('button', {
      text: 'Edit',
      onclick: (event) => {
        edit.hidden = !edit.hidden;
        event.currentTarget.textContent = edit.hidden ? 'Edit' : 'Original';
        allow.textContent = edit.hidden ? 'Allow' : 'Allow edited';
      },
    }));
  }
  actions.append(h('button', { class: 'ghost', text: 'Terminal', onclick: () => decide(request, { behavior: 'terminal' }, card) }));
  card.append(actions);
  return card;
}

function toolDetails(request) {
  const input = request.tool_input || {};
  switch (request.tool_name) {
    case 'Bash':
      return [
        input.description ? h('p', { class: 'muted', text: input.description }) : null,
        h('pre', { class: 'code', text: input.command || '' }),
      ];
    case 'Edit':
      return [
        h('code', { text: input.file_path || '' }),
        h('pre', { class: 'code diff-del', text: input.old_string || '' }),
        h('pre', { class: 'code diff-add', text: input.new_string || '' }),
      ];
    case 'MultiEdit':
      return [
        h('code', { text: input.file_path || '' }),
        ...(input.edits || []).flatMap((edit) => [
          h('pre', { class: 'code diff-del', text: edit.old_string || '' }),
          h('pre', { class: 'code diff-add', text: edit.new_string || '' }),
        ]),
      ];
    case 'Write':
      return [h('code', { text: input.file_path || '' }), h('pre', { class: 'code diff-add', text: input.content || '' })];
    case 'WebFetch':
      return [h('code', { text: input.url || '' }), input.prompt ? h('p', { class: 'muted', text: input.prompt }) : null];
    default:
      return [h('pre', { class: 'code', text: JSON.stringify(input, null, 2) })];
  }
}

function questionForm(request, card) {
  const questions = (request.tool_input && request.tool_input.questions) || [];
  const form = h('form', { class: 'stack' });
  const fields = questions.map((question, index) => {
    const name = `q${request.id}-${index}`;
    const type = question.multiSelect ? 'checkbox' : 'radio';
    const other = h('input', { type: 'text', placeholder: 'Other answer' });
    const fieldset = h('fieldset', {},
      h('legend', { text: question.header || `Question ${index + 1}` }),
      h('p', { text: question.question }),
      ...(question.options || []).map((option) => h('label', { class: 'option' },
        h('input', { type, name, value: option.label }),
        h('span', {}, option.label, option.description ? h('small', { text: option.description }) : null))),
      other);
    return { question, fieldset, other, name };
  });
  form.append(...fields.map((field) => field.fieldset));
  const error = h('p', { class: 'error' });
  form.append(error, h('div', { class: 'actions' },
    h('button', { class: 'primary', type: 'submit', text: 'Answer' }),
    h('button', { class: 'ghost', type: 'button', text: 'Terminal', onclick: () => decide(request, { behavior: 'terminal' }, card) })));
  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const answers = {};
    for (const field of fields) {
      const picked = [...field.fieldset.querySelectorAll(`input[name="${field.name}"]:checked`)].map((input) => input.value);
      const other = field.other.value.trim();
      if (other) picked.push(other);
      if (!picked.length) {
        error.textContent = `Answer “${field.question.header || field.question.question}”.`;
        return;
      }
      answers[field.question.question] = field.question.multiSelect ? picked : picked[picked.length - 1];
    }
    decide(request, { behavior: 'answer', answers }, card);
  });
  return form;
}

/* ---------- Agents ---------- */

function renderAgents() {
  const root = $('tab-agents');
  root.replaceChildren();
  if (state.herdrError) {
    if (!state.sessions.length) {
      root.append(h('p', { class: 'empty', text: 'No agents to show yet.' }));
      return;
    }
    for (const session of state.sessions) {
      root.append(h('button', {
        class: 'row',
        onclick: () => openPanel({ paneId: session.pane_id, sessionId: session.session_id, title: basename(session.cwd) || 'Session' }),
      },
      h('span', { class: 'agent-dot' }),
      h('span', { class: 'row-main' },
        h('strong', { text: basename(session.cwd) || session.session_id.slice(0, 8) }),
        h('span', { text: session.cwd || '' }))));
    }
    return;
  }
  if (!state.agents.length) {
    root.append(h('p', { class: 'empty', text: 'No agents are running. Start one with ＋.' }));
    return;
  }
  const agents = [...state.agents].sort((a, b) => (STATUS_ORDER[a.agent_status] - STATUS_ORDER[b.agent_status]) || agentLabel(a).localeCompare(agentLabel(b)));
  for (const agent of agents) {
    const label = agent.state_labels && agent.state_labels[agent.agent_status];
    const activity = state.activity.get(agent.pane_id);
    const pending = state.requests.filter((request) => request.pane_id === agent.pane_id).length;
    root.append(h('button', {
      class: 'row',
      onclick: () => openPanel({ paneId: agent.pane_id, sessionId: agent.session_id, title: agentLabel(agent) }),
    },
    h('span', { class: `agent-dot ${agent.agent_status}`, title: agent.agent_status }),
    h('span', { class: 'row-main' },
      h('strong', { text: agentLabel(agent) }),
      h('span', { text: joinParts([agent.workspace_label, basename(agent.cwd), label || agent.agent_status]) }),
      activity && agent.agent_status === 'working' ? h('span', { text: activity }) : null),
    pending ? h('span', { class: 'badge', text: String(pending) }) : null));
  }
}

function renderBanner() {
  const banner = $('banner');
  banner.hidden = !state.herdrError;
  banner.textContent = state.herdrError ? `Herdr is not reachable (${state.herdrError}). Showing sessions seen by hooks.` : '';
}

/* ---------- Activity ---------- */

function renderActivity() {
  if (state.tab !== 'activity') return;
  const root = $('tab-activity');
  root.replaceChildren();
  const events = state.events.slice(-150).reverse();
  if (!events.length) {
    root.append(h('p', { class: 'empty', text: 'No activity yet.' }));
    return;
  }
  for (const event of events) {
    root.append(h('div', { class: 'event' },
      h('div', { text: describe(event) }),
      h('span', { text: originLabel(event) })));
  }
}

/* ---------- Agent panel ---------- */

function openPanel(panel) {
  state.panel = panel;
  state.panelView = 'chat';
  $('agent').hidden = false;
  $('agent-chat').replaceChildren();
  selectPanelView('chat');
  renderPanelHeader();
  renderPanelActivity();
  refreshChat();
}

function closePanel() {
  state.panel = null;
  $('agent').hidden = true;
}

function renderPanelHeader() {
  const panel = state.panel;
  if (!panel) return;
  const agent = panel.paneId ? agentFor(panel.paneId) : null;
  if (agent && !panel.sessionId && agent.session_id) panel.sessionId = agent.session_id;
  $('agent-name').textContent = agent ? agentLabel(agent) : panel.title;
  $('agent-sub').textContent = agent
    ? joinParts([agent.workspace_label, basename(agent.cwd), agent.agent_status])
    : 'Not in a Herdr pane';
  const canType = Boolean(panel.paneId);
  $('composer-text').disabled = !canType;
  $('agent-interrupt').disabled = !canType;
  document.querySelector('[data-view="screen"]').disabled = !canType;
}

function renderPanelActivity() {
  const panel = state.panel;
  const line = $('agent-activity');
  const activity = panel && panel.paneId ? state.activity.get(panel.paneId) : null;
  line.hidden = !activity;
  line.textContent = activity ? `Running ${activity}` : '';
}

function selectPanelView(view) {
  state.panelView = view;
  for (const button of document.querySelectorAll('[data-view]')) button.classList.toggle('active', button.dataset.view === view);
  $('agent-chat').hidden = view !== 'chat';
  $('agent-screen').hidden = view !== 'screen';
  if (view === 'screen') refreshScreen();
}

async function refreshChat() {
  const panel = state.panel;
  if (!panel || state.panelView !== 'chat') return;
  const chat = $('agent-chat');
  if (!panel.sessionId) {
    chat.replaceChildren(h('p', { class: 'empty', text: 'No chat yet. It appears once this agent’s Claude Code hooks report its session. Use Screen meanwhile.' }));
    return;
  }
  let data;
  try {
    data = await api(`/v1/sessions/${encodeURIComponent(panel.sessionId)}/transcript?limit=200`);
  } catch (error) {
    if (state.panel !== panel) return;
    chat.replaceChildren(h('p', { class: 'empty', text: error.status === 404 ? 'No transcript is known for this session yet.' : `Could not load the chat: ${error.message}` }));
    return;
  }
  if (state.panel !== panel) return;
  const pinned = chat.scrollHeight - chat.scrollTop - chat.clientHeight < 80;
  const results = new Map(data.entries.filter((entry) => entry.kind === 'tool_result').map((entry) => [entry.tool_use_id, entry]));
  const nodes = [];
  for (const entry of data.entries) {
    if (entry.kind === 'user') nodes.push(h('div', { class: 'bubble user', text: entry.text }));
    else if (entry.kind === 'assistant') nodes.push(h('div', { class: 'bubble assistant', text: entry.text }));
    else if (entry.kind === 'tool_use') {
      const result = results.get(entry.id);
      nodes.push(h('details', { class: `tool${result && result.is_error ? ' failed' : ''}` },
        h('summary', { text: `⚙ ${entry.name} ${entry.summary}` }),
        result ? h('pre', { text: result.text || '(no output)' }) : h('pre', { text: 'Running…' })));
    }
  }
  if (!nodes.length) nodes.push(h('p', { class: 'empty', text: 'The conversation is empty so far.' }));
  chat.replaceChildren(...nodes);
  if (pinned) chat.scrollTop = chat.scrollHeight;
}

async function refreshScreen() {
  const panel = state.panel;
  if (!panel || !panel.paneId || state.panelView !== 'screen') return;
  try {
    const data = await api(`/v1/panes/${encodeURIComponent(panel.paneId)}/screen`);
    if (state.panel === panel) $('screen-text').textContent = data.text;
  } catch (error) {
    if (state.panel === panel) $('screen-text').textContent = `Could not read the screen: ${error.message}`;
  }
}

async function sendKeys(keys) {
  const panel = state.panel;
  if (!panel || !panel.paneId) return;
  try {
    await api(`/v1/panes/${encodeURIComponent(panel.paneId)}/keys`, { method: 'POST', body: { keys } });
    setTimeout(refreshScreen, 250);
  } catch (error) {
    toast(`Key not sent: ${error.message}`);
  }
}

/* ---------- Sheets ---------- */

function openSheet(...content) {
  const sheet = $('sheet');
  sheet.replaceChildren(...content);
  sheet.showModal();
}

async function openNewAgent() {
  const workspaces = await api('/v1/workspaces').then((data) => data.workspaces).catch(() => []);
  const kind = h('select', {}, ...KINDS.map((value) => h('option', { value, text: value })));
  const name = h('input', { required: true, pattern: '[a-z][a-z0-9_\\-]{0,31}', placeholder: 'api-fix', autocapitalize: 'off', autocomplete: 'off', spellcheck: 'false' });
  const where = h('select', {},
    h('option', { value: '', text: 'Focused workspace' }),
    ...workspaces.map((workspace) => h('option', { value: workspace.workspace_id, text: workspace.label })));
  const cwd = h('input', { placeholder: 'Directory (optional)', autocapitalize: 'off', spellcheck: 'false' });
  const repo = h('input', { placeholder: '/path/to/repo', autocapitalize: 'off', spellcheck: 'false' });
  const branch = h('input', { placeholder: 'branch-name', autocapitalize: 'off', spellcheck: 'false' });
  const tabFields = h('div', { class: 'stack' },
    h('label', {}, 'Workspace', where), h('label', {}, 'Directory', cwd));
  const worktreeFields = h('div', { class: 'stack', hidden: true },
    h('label', {}, 'Repository', repo), h('label', {}, 'New branch', branch));
  const mode = (value) => h('label', {},
    h('input', {
      type: 'radio',
      name: 'placement',
      value,
      checked: value === 'tab',
      onchange: () => {
        tabFields.hidden = value !== 'tab';
        worktreeFields.hidden = value !== 'worktree';
      },
    }),
    value === 'tab' ? 'New tab' : 'New worktree');
  const error = h('p', { class: 'error' });
  const submit = h('button', { class: 'primary', type: 'submit', text: 'Start agent' });
  const form = h('form', {},
    h('h2', { text: 'Start an agent' }),
    h('label', {}, 'Agent', kind),
    h('label', {}, 'Name (lowercase, unique)', name),
    h('div', { class: 'choice' }, mode('tab'), mode('worktree')),
    tabFields, worktreeFields, error,
    h('div', { class: 'actions' }, submit, h('button', { type: 'button', class: 'ghost', text: 'Cancel', onclick: () => $('sheet').close() })));
  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const worktree = !worktreeFields.hidden;
    if (worktree && (!repo.value.trim() || !branch.value.trim())) {
      error.textContent = 'A worktree needs a repository and a branch.';
      return;
    }
    const placement = worktree
      ? { in: 'worktree', cwd: repo.value.trim(), branch: branch.value.trim() }
      : { in: 'tab', workspace_id: where.value || null, cwd: cwd.value.trim() || null };
    submit.disabled = true;
    submit.textContent = 'Starting…';
    error.textContent = '';
    try {
      const data = await api('/v1/agents', { method: 'POST', body: { kind: kind.value, name: name.value, placement } });
      $('sheet').close();
      await refreshAgents();
      openPanel({ paneId: data.agent.pane_id, sessionId: data.agent.session_id, title: agentLabel(data.agent) });
    } catch (failure) {
      error.textContent = failure.message;
      submit.disabled = false;
      submit.textContent = 'Start agent';
    }
  });
  openSheet(form);
  name.focus();
}

function openSettings() {
  openSheet(h('form', { method: 'dialog' },
    h('h2', { text: 'Settings' }),
    h('p', { class: 'muted', text: `Connected to ${location.host}. Install this page to the home screen from the browser’s share menu to use it like an app.` }),
    h('p', { class: 'muted', text: 'For alerts while the app is closed, start the companion with --ntfy and subscribe to that topic in the ntfy app.' }),
    h('div', { class: 'actions' },
      h('button', {
        class: 'danger',
        type: 'button',
        text: 'Forget token',
        onclick: () => {
          writeToken('');
          state.token = '';
          $('sheet').close();
          showSetup();
        },
      }),
      h('button', { class: 'primary', text: 'Done' }))));
}

/* ---------- Wiring ---------- */

function selectTab(tab) {
  state.tab = tab;
  for (const button of document.querySelectorAll('[data-tab]')) button.classList.toggle('active', button.dataset.tab === tab);
  for (const name of ['inbox', 'agents', 'activity']) $(`tab-${name}`).hidden = name !== tab;
  $('title').textContent = { inbox: 'Inbox', agents: 'Agents', activity: 'Activity' }[tab];
  if (tab === 'agents') refreshAgents();
  if (tab === 'activity') renderActivity();
}

async function start() {
  if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});

  $('setup-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    if (await connect($('setup-token').value.trim())) boot();
  });
  for (const button of document.querySelectorAll('[data-tab]')) button.addEventListener('click', () => selectTab(button.dataset.tab));
  for (const button of document.querySelectorAll('[data-view]')) button.addEventListener('click', () => selectPanelView(button.dataset.view));
  $('agent-back').addEventListener('click', closePanel);
  $('new-agent').addEventListener('click', () => openNewAgent());
  $('settings').addEventListener('click', openSettings);
  $('agent-interrupt').addEventListener('click', async () => {
    const panel = state.panel;
    if (!panel || !panel.paneId) return;
    try {
      await api(`/v1/panes/${encodeURIComponent(panel.paneId)}/interrupt`, { method: 'POST' });
      toast('Interrupt sent.');
    } catch (error) {
      toast(`Could not interrupt: ${error.message}`);
    }
  });
  $('keypad').append(...KEYS.map(([label, key]) => h('button', { type: 'button', text: label, onclick: () => sendKeys([key]) })));
  const composer = $('composer-text');
  composer.addEventListener('input', () => {
    composer.style.height = 'auto';
    composer.style.height = `${composer.scrollHeight}px`;
  });
  $('composer').addEventListener('submit', async (event) => {
    event.preventDefault();
    const panel = state.panel;
    const text = composer.value.trim();
    if (!panel || !panel.paneId || !text) return;
    composer.disabled = true;
    try {
      await api(`/v1/panes/${encodeURIComponent(panel.paneId)}/prompt`, { method: 'POST', body: { text } });
      composer.value = '';
      composer.style.height = 'auto';
    } catch (error) {
      toast(`Not sent: ${error.message}`);
    } finally {
      composer.disabled = false;
    }
  });

  const linked = tokenFromLink();
  if (linked) state.token = linked;
  if (state.token && (await connect(state.token))) boot();
  else showSetup(linked ? 'The token in that link was refused.' : '');
  pollLoop();
  // Herdr status changes (working, idle) do not pass through the hooks.
  setInterval(() => {
    if (document.visibilityState !== 'visible' || !$('setup').hidden) return;
    if (state.tab === 'agents' || state.panel) refreshAgents();
    if (state.panel && state.panelView === 'screen') refreshScreen();
  }, 4000);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && $('setup').hidden) {
      refreshRequestsSoon();
      refreshAgentsSoon();
    }
  });
}

async function boot() {
  const page = await api('/v1/events?after=0').catch(() => null);
  if (page) {
    state.cursor = page.next;
    state.events = page.events.slice(-300);
    state.events.forEach(trackActivity);
  }
  await Promise.all([refreshRequests().catch(() => {}), refreshAgents()]);
  renderActivity();
}

start();
