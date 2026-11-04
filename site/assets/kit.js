// Illustrative app renderings for the site. These are drawn in HTML, not
// screenshots; real captures go in assets/shots (see site/README.md). Usage:
//   <div data-hm-app="orchestrator" data-theme="light" style="--h:440px"></div>
//   <div data-hm-video></div>          (optional data-poster="devices")
//   HM.SHOTS, HM.CHAPTERS, HM.PILLARS, HM.FACTS for copy; HM.lightbox(tiles) for galleries.
(function () {
  const A = (n) => `assets/agent-${n}.svg`;
  const I = (n) => `assets/icon-${n}.svg`;

  const FACTS = {
    disclaimer: 'Independent project, not affiliated with or supported by Herdr or Herdr Code.',
    license: 'Apache-2.0',
    oneLiner: 'Sheprd: a native workspace for coding agents, with tools to dispatch, review and move work between machines.',
  };

  // The product story. Pages may use all or some.
  const PILLARS = [
    { key: 'dispatch', title: 'Dispatch', line: 'Turn issues, beads and PRs into agent runs. Fan one prompt out to several agents, on the host with the most room, and keep the best.' },
    { key: 'watch', title: 'Watch', line: 'Agent status, host load, ports and recent activity. Local and SSH devices, Coder workspaces on macOS/Linux, and WSL on Windows.' },
    { key: 'review', title: 'Review', line: 'Read an agent’s changes side by side, pin line notes and send them back. Checkpoint every turn and rewind a worktree. Review and merge PRs.' },
    { key: 'move', title: 'Move', line: 'Teleport a worktree, its uncommitted work, tabs and running agents to another machine. Forward ports. Pick up where you left off.' },
    { key: 'work', title: 'Work', line: 'GPU-painted terminals, VS Code and browser tabs in editor groups, Go to File and Symbol, annotations on pages and terminal text.' },
  ];

  const CHAPTERS = [
    ['0:00', 'One window, every agent'],
    ['0:12', 'Dispatch from an issue'],
    ['0:26', 'Fan out, keep the best'],
    ['0:40', 'Review, notes back'],
    ['0:54', 'Teleport'],
    ['1:08', 'Close it. Still running.'],
  ];

  // Canonical capture briefs for Sheprd's gallery.
  const SHOTS = [
    { id: 'orchestrator', pillar: 'dispatch', title: 'Orchestrator', cap: 'Issues, beads, PRs and agent runs for a repository, in one tab.', spec: 'Orchestrator tab: Issues list with beads nested, preview pane on the right, "Dispatch agent" button.' },
    { id: 'dispatch', pillar: 'dispatch', title: 'Dispatch an agent', cap: 'Prompt, agent, model, branch, and host selection.', spec: 'Dispatch dialog: prompt, agent picker (Claude), model, branch agent/412-..., three host tiles ranked by spare cores.' },
    { id: 'fanout', pillar: 'dispatch', title: 'Fan out, keep the best', cap: 'One prompt, several agents, lanes spread across hosts.', spec: 'Fan-out: 4 lanes (Claude, Codex, OpenCode, Gemini) with host chips, diff sizes, "Keep" on the winner.' },
    { id: 'devices', pillar: 'watch', title: 'Devices overview', cap: 'Recent agent activity and the load of every machine.', spec: 'Devices tab: activity heatmap, totals, device table with dots, CPU, memory, disk, uptime.' },
    { id: 'sidebar', pillar: 'watch', title: 'Every host, one sidebar', cap: 'Local and SSH devices, coloured per host, with platform-specific cloud and WSL integrations.', spec: 'Sidebar: 4 host groups with pinned headers, load bars, ahead/behind counts, worktree notes.' },
    { id: 'review', pillar: 'review', title: 'Review with line notes', cap: 'Side-by-side diff; notes go back to the agent.', spec: 'Review tab: file tree, language-coloured split diff, two line notes, "Send to agent".' },
    { id: 'checkpoints', pillar: 'review', title: 'Checkpoints', cap: 'Save checkpoints and restore your worktree.', spec: 'Checkpoint timeline with clock times and a day break, restore confirmation.' },
    { id: 'pr', pillar: 'review', title: 'Pull requests', cap: 'Conversation, checks, review and merge, without leaving.', spec: 'PR page: avatars, review threads folded, checks green, Merge (squash) button.' },
    { id: 'teleport', pillar: 'move', title: 'Teleport', cap: 'Move a worktree and its agent sessions to another machine.', spec: 'Teleport review: branch, 3 unpushed commits, 5 files, panes and agent sessions, destination build-box.' },
    { id: 'ports', pillar: 'move', title: 'Ports and tunnels', cap: 'Each workspace’s listening ports, opened locally through SSH.', spec: 'Ports popover: :3000 vite, :5432 postgres on build-box, "Open in tab" via tunnel.' },
    { id: 'vscode', pillar: 'work', title: 'VS Code in an editor group', cap: 'Beside the agent, in the same window.', spec: 'Editor groups: VS Code tab left, Claude terminal right, Go to Symbol palette open.' },
    { id: 'annotate', pillar: 'work', title: 'Annotate pages and terminal text', cap: 'Pin notes on a page or a terminal selection; the agent gets them.', spec: 'Browser tab with 3 pins + terminal selection with a note bubble.' },
    { id: 'cells', pillar: 'work', title: 'GPU-painted cells', cap: 'Pixel-aligned terminal glyphs, joined box drawing and smooth scrolling.', spec: 'Macro crop at 2x: terminal borders and Cmd-F find bar.' },
    { id: 'usage', pillar: 'watch', title: 'Usage in the status bar', cap: 'Claude, Codex, ChatGPT and more, before an agent stalls.', spec: 'Status bar meters + provider popover with several Claude accounts.' },
  ];

  function sidebar() {
    return `<div class="hm-sd">
      <div class="hm-search">⌕ Search devices, worktrees…</div>
      <div class="hm-h"><span style="color:#3dd6d0">● MacBook</span><span>18%<span class="bar"><i style="width:18%"></i></span></span></div>
      <div class="hm-r on"><b>sheprd</b><span class="s"><span class="cg">↑2</span></span></div>
      <div class="hm-r ind"><img src="${A('claude')}">sidebar-polish<span class="s"><span class="hm-dot work"></span></span></div>
      <div class="hm-r ind"><img src="${A('codex')}">agent/412-ports<span class="s"><span class="hm-dot block"></span></span></div>
      <div class="hm-h"><span style="color:#c792ea">● build-box · ssh</span><span>64%<span class="bar"><i style="width:64%"></i></span></span></div>
      <div class="hm-r"><b>arbor</b><span class="s"><span class="cd">↓1</span></span></div>
      <div class="hm-r ind"><img src="${A('opencode')}">fan-out · lane 2<span class="s"><span class="hm-dot work"></span></span></div>
      <div class="hm-r ind"><img src="${A('gemini')}">fan-out · lane 3<span class="s"><span class="hm-dot done"></span></span></div>
      <div class="hm-h"><span style="color:#f2c14e">● coder · dev-ws</span><span>31%<span class="bar"><i style="width:31%"></i></span></span></div>
      <div class="hm-r"><b>moltis</b><span class="s">✎</span></div>
      <div class="hm-r ind"><img src="${A('copilot')}">bench-gpu<span class="s"><span class="hm-dot work"></span></span></div>
      <div class="hm-foot"><span>⌂ All Devices</span><span>5 working · 1 blocked</span></div>
    </div>`;
  }

  const TABS = {
    terminal: [['claude', 'sidebar-polish', 1], ['codex', 'agent/412-ports'], [null, '🌐 localhost:3000'], [null, '⧉ VS Code']],
    devices: [[null, '▦ Devices', 1], ['claude', 'sidebar-polish']],
    orchestrator: [[null, '◎ Orchestrator · sheprd', 1], ['claude', 'sidebar-polish']],
    review: [[null, '± Review · sidebar-polish', 1], ['claude', 'sidebar-polish']],
    checkpoints: [['claude', 'sidebar-polish', 1], [null, '± Review']],
    fanout: [[null, '⑂ Fan-out · "fix flaky ports test"', 1]],
    teleport: [['claude', 'sidebar-polish', 1]],
  };

  const STATUS = `<div class="hm-st"><span class="hm-dot done"></span>MacBook · 3 hosts<span class="m">Claude 5h <span><i style="width:62%"></i></span>62%</span><span class="m">Codex <span><i style="width:28%"></i></span>28%</span><span>:3000 :5432</span><span style="margin-left:auto">⌘K</span></div>`;

  const VIEWS = {
    terminal: () => `<div class="hm-panes"><div class="hm-pn"><span class="cm">✻ Claude Code</span>  <span class="cd">sidebar-polish</span>

<span class="cd">›</span> tighten host rows in compact mode

<span class="cy">●</span> Read <span class="cb">src/sidebar/rows.rs</span>
<span class="cy">●</span> Update <span class="cb">rows.rs</span>
  <span class="cg">+    .gap(px(6.))</span>
  <span class="cr">-    .gap(px(10.))</span>
<span class="cy">●</span> Bash <span class="cd">just ci</span>
  <span class="cd">⎿ test result: ok. 1,284 passed</span>

<span class="cy">✶ Thinking…</span></div><div class="hm-pn"><span class="cb">sheprd</span> <span class="cg">main</span> <span class="cd">❯</span> just run
<span class="cd">   Compiling sheprd-gpui</span>
<span class="cg">    Finished</span> release
<span class="cd">     Running</span> sheprd

<span class="cb">sheprd</span> <span class="cg">main</span> <span class="cd">❯</span> gh pr checks
<span class="cg">✓</span> Format, lint, and test
<span class="cg">✓</span> windows
<span class="cy">●</span> release-dry-run

<span class="cb">sheprd</span> <span class="cg">main</span> <span class="cd">❯</span> <span style="background:var(--fg)"> </span></div></div>`,

    devices: () => {
      const cell = (i) => { const r = (Math.sin(i * 1.7) + Math.sin(i * .31)) * .5 + .5; return r > .975 ? '#e5484d' : `rgba(61,214,208,${(.12 + r * .75).toFixed(2)})`; };
      const heat = Array.from({ length: 180 }, (_, i) => `<i style="background:${cell(i)}"></i>`).join('');
      const row = (n, c, dots, cpu, mem, disk) => `<tr><td><span style="color:${c}">●</span> ${n}</td><td>${dots}</td><td>${cpu}</td><td>${mem}</td><td>${disk}</td></tr>`;
      const d = (s) => s.split('').map(k => `<span class="hm-dot ${{ w: 'work', b: 'block', d: 'done', i: 'idle' }[k]}" style="margin-right:3px"></span>`).join('');
      return `<div class="hm-view"><h5>Devices <small>last 2 hours · 4 online</small></h5>
        <div class="hm-card" style="margin-bottom:10px"><div style="display:flex;justify-content:space-between;color:var(--dim);font-size:11px;margin-bottom:6px"><span>Agent activity</span><span><b style="color:var(--fg)">5</b> working · <b class="cr">1</b> blocked · peak 9</span></div><div class="hm-heat">${heat}</div></div>
        <table class="hm-tbl"><tr><th>Device</th><th>Agents</th><th>CPU</th><th>Memory</th><th>Disk free</th></tr>
        ${row('MacBook', '#3dd6d0', d('wb'), '18%', '21 / 64 GB', '412 GB')}
        ${row('build-box · ssh', '#c792ea', d('wwd'), '64%', '48 / 128 GB', '1.2 TB')}
        ${row('coder · dev-ws', '#f2c14e', d('w'), '31%', '6 / 16 GB', '—')}
        ${row('daytona · sbx-7', '#7cb7ff', d('i'), '—', '—', '—')}</table></div>`;
    },

    orchestrator: () => `<div class="hm-view" style="display:grid;grid-template-columns:1.25fr 1fr;gap:12px;padding:14px">
      <div><div style="display:flex;gap:6px;margin-bottom:10px"><span class="hm-pill on">Issues 23</span><span class="hm-pill">Pull requests 7</span><span class="hm-pill">Runs 5</span><span class="hm-pill" style="margin-left:auto">⌕ ports</span></div>
        <table class="hm-tbl">
        <tr><td><span class="cg">◉</span> #412 Keep listening ports per session</td><td><span class="hm-dot work"></span></td></tr>
        <tr><td style="padding-left:22px"><span class="cd">◇</span> bd-7a Scan remote loopback ports</td><td><span class="hm-dot block"></span></td></tr>
        <tr style="background:var(--sel)"><td><span class="cg">◉</span> #409 Fan-out lanes across hosts</td><td></td></tr>
        <tr><td><span class="cg">◉</span> #401 Checkpoint timeline day breaks</td><td><span class="hm-dot done"></span></td></tr>
        <tr><td><span class="cg">◉</span> #398 WSL device setup</td><td></td></tr>
        <tr><td><span class="cg">◉</span> #395 VS Code in editor groups</td><td><span class="hm-dot done"></span></td></tr></table></div>
      <div class="hm-card"><b>#409 Fan-out lanes across hosts</b><p style="color:var(--dim);margin:6px 0 10px">Spread lanes to the host with the most spare cores; show the host chip on each lane.</p>
        <div style="font-size:11px;color:var(--dim);margin-bottom:5px">Dispatch to</div>
        <div class="hm-grid2" style="grid-template-columns:repeat(3,1fr);gap:6px;margin-bottom:10px">
          <div class="hm-card" style="padding:6px 8px;border-color:var(--acc)"><b>build-box</b><br><span class="cd">22 cores free</span></div>
          <div class="hm-card" style="padding:6px 8px"><b>coder</b><br><span class="cd">6 free</span></div>
          <div class="hm-card" style="padding:6px 8px"><b>MacBook</b><br><span class="cd">3 free</span></div></div>
        <div style="display:flex;gap:6px"><span class="hm-pill on">▶ Dispatch agent</span><span class="hm-pill">⑂ Fan out</span></div></div></div>`,

    review: () => `<div class="hm-view" style="display:grid;grid-template-columns:150px 1fr;gap:12px;padding:12px">
      <div style="font-size:11.5px;color:var(--dim);line-height:1.9">▾ crates/sheprd-gpui<br>&nbsp;&nbsp;▾ sidebar<br>&nbsp;&nbsp;&nbsp;&nbsp;<span style="color:var(--fg)">rows.rs</span> <span class="cg">+6</span> <span class="cr">-4</span><br>&nbsp;&nbsp;&nbsp;&nbsp;layout.rs <span class="cg">+2</span><br>&nbsp;&nbsp;▸ status_bar<br>▸ docs</div>
      <div class="hm-diff"><span class="n"> 88  fn host_row(cx: &amp;RowContext) -&gt; Div {</span>
<span class="n"> 89      h_flex()</span>
<span class="d"> 90 -        .gap(px(10.))</span>
<span class="a"> 90 +        .gap(px(6.))</span>
<span class="a"> 91 +        .when(cx.compact, |d| d.py(px(2.)))</span>
<div class="cm"><b>Note</b> · keep 8px when a load bar is shown, or the percent clips.</div><span class="n"> 92          .child(cx.host_label())</span>
<span class="n"> 93          .child(load_bar(cx))</span>
<div style="margin-top:8px;display:flex;gap:6px;font-family:-apple-system,sans-serif"><span class="hm-pill on">Send 1 note to Claude</span><span class="hm-pill">Checkpoints ◷</span></div></div></div>`,

    checkpoints: () => `<div class="hm-view"><h5>Checkpoints <small>sidebar-polish · Claude</small></h5>
      ${[['14:52', 'Tighten compact host rows', 'cur'], ['14:41', 'Pin host headers while scrolling'], ['14:30', 'Colour each host group'], ['—', 'Yesterday', 'day'], ['18:07', 'Add load bar to host header'], ['17:55', 'Start: sidebar polish']].map(([t, m, k]) =>
        k === 'day' ? `<div style="color:var(--dim);font-size:11px;margin:8px 0 6px;border-top:1px dashed var(--ln);padding-top:6px">${m}</div>` :
        `<div style="display:flex;gap:10px;align-items:center;padding:6px 0"><span style="width:10px;height:10px;border-radius:50%;border:2px solid var(--acc);${k ? 'background:var(--acc)' : ''}"></span><span class="cd" style="font-family:ui-monospace,monospace">${t}</span><span>${m}</span>${k ? '<span class="hm-pill" style="margin-left:auto">current</span>' : '<span class="hm-pill" style="margin-left:auto">Restore</span>'}</div>`).join('')}</div>`,

    fanout: () => `<div class="hm-view"><h5>Fan-out <small>"fix the flaky ports test" · 4 lanes · spread across hosts</small></h5>
      <div class="hm-grid2">${[['claude', 'Claude', 'build-box', 'done', '+42 −18', 1], ['codex', 'Codex', 'build-box', 'done', '+97 −30'], ['opencode', 'OpenCode', 'coder', 'work', '+12 −3'], ['gemini', 'Gemini', 'MacBook', 'block', '+5 −0']].map(([a, n, h, s, d, best]) =>
        `<div class="hm-card" style="${best ? 'border-color:var(--acc)' : ''}"><div style="display:flex;align-items:center;gap:8px"><img src="${A(a)}" width="16"><b>${n}</b><span class="hm-pill">${h}</span><span class="hm-dot ${s}" style="margin-left:auto"></span></div>
        <div class="cd" style="margin:8px 0;font-family:ui-monospace,monospace">${d} · tests ${s === 'done' ? '<span class="cg">pass</span>' : '…'}</div>${best ? '<span class="hm-pill on">✓ Keep this one</span>' : '<span class="hm-pill">Compare</span>'}</div>`).join('')}</div></div>`,

    teleport: () => `<div class="hm-view"><h5><img src="${I('teleport')}" width="16" style="filter:invert(.6)"> Teleport sidebar-polish <small>MacBook → build-box</small></h5>
      <div class="hm-grid2"><div class="hm-card"><b>Git</b><div class="cd" style="margin-top:6px;line-height:1.8">branch sidebar-polish<br>3 unpushed commits<br>5 changed · 2 untracked</div></div>
      <div class="hm-card"><b>Panes</b><div class="cd" style="margin-top:6px;line-height:1.8"><img src="${A('claude')}" width="12"> Claude → resumes session<br>just run → reruns<br>2 tabs · 3 splits</div></div></div>
      <div style="margin-top:12px;display:flex;gap:8px;align-items:center"><span class="hm-pill">build-box · 22 cores free</span><span class="hm-pill on" style="margin-left:auto">Teleport ⏎</span></div></div>`,
  };

  function app(el) {
    const view = el.dataset.hmApp || 'terminal';
    const tabs = (TABS[view] || TABS.terminal).map(([a, t, on]) => `<span class="hm-tab${on ? ' on' : ''}">${a ? `<img src="${A(a)}">` : ''}${t}</span>`).join('');
    el.innerHTML = `<div class="hm-app ${el.dataset.theme === 'light' ? 'light' : ''}">
      <div class="hm-tb"><div class="l"><i></i><i></i><i></i><span class="nav">‹›</span></div><div class="hm-tabs">${tabs}</div></div>
      <div class="hm-bd">${el.dataset.noSidebar !== undefined ? '' : sidebar()}<div class="hm-mn">${(VIEWS[view] || VIEWS.terminal)()}${STATUS}</div></div></div>`;
    if (el.dataset.noSidebar !== undefined) el.querySelector('.hm-bd').style.gridTemplateColumns = '1fr', el.querySelector('.hm-tb').style.gridTemplateColumns = '120px 1fr';
  }

  function video(el) {
    const poster = el.dataset.poster;
    el.innerHTML = `<div class="hm-video">${poster ? `<div class="poster" data-hm-app="${poster}" data-theme="${el.dataset.theme || ''}"></div>` : ''}
      <span class="tag">${el.dataset.label || 'SHOWCASE VIDEO · 1:20 · MOTION GRAPHIC PLACEHOLDER'}</span><span class="play" title="Play"></span>
      ${el.dataset.noChapters !== undefined ? '' : `<div class="chs">${CHAPTERS.map(([t, n], i) => `<span class="ch${i ? '' : ' on'}"><i></i><small>${t}</small>${n}</span>`).join('')}</div>`}</div>`;
    el.querySelectorAll('.ch').forEach(c => c.onclick = () => el.querySelectorAll('.ch').forEach(x => x.classList.toggle('on', x === c)));
  }

  function shot(s) {
    return `<div class="hm-shot" data-shot="${s.id}"><span class="k">${s.title.toUpperCase()}</span>${s.spec}</div>`;
  }

  // Wire any [data-shot] elements on the page to a shared lightbox, in document order.
  function lightbox() {
    const tiles = [...document.querySelectorAll('[data-shot]')];
    if (!tiles.length) return;
    const lb = document.createElement('div');
    lb.className = 'hm-lb';
    lb.innerHTML = '<span class="nv pv">‹</span><div class="in"></div><span class="nv nx">›</span>';
    document.body.appendChild(lb);
    let i = 0;
    const show = (n) => {
      i = (n + tiles.length) % tiles.length;
      const s = SHOTS.find(x => x.id === tiles[i].dataset.shot) || { title: '', cap: '', spec: '' };
      lb.querySelector('.in').innerHTML = shot(s) + `<div class="cap"><b>${s.title}</b><span>${i + 1} / ${tiles.length} · ${s.cap}</span></div>`;
      lb.classList.add('on');
    };
    tiles.forEach((t, n) => { t.style.cursor = 'zoom-in'; t.addEventListener('click', () => show(n)); });
    lb.querySelector('.pv').onclick = (e) => { e.stopPropagation(); show(i - 1); };
    lb.querySelector('.nx').onclick = (e) => { e.stopPropagation(); show(i + 1); };
    lb.onclick = () => lb.classList.remove('on');
    document.addEventListener('keydown', (e) => {
      if (!lb.classList.contains('on')) return;
      if (e.key === 'Escape') lb.classList.remove('on');
      if (e.key === 'ArrowRight') show(i + 1);
      if (e.key === 'ArrowLeft') show(i - 1);
    });
  }

  function mountAll(root = document) {
    root.querySelectorAll('[data-hm-video]').forEach(video);
    root.querySelectorAll('[data-hm-app]').forEach(app);
  }

  window.HM = { FACTS, PILLARS, CHAPTERS, SHOTS, A, I, app, video, shot, lightbox, mountAll };
  document.addEventListener('DOMContentLoaded', () => { mountAll(); lightbox(); });
})();
