// Page behavior for Sheprd. kit.js mounts the illustrations on DOMContentLoaded.
(function () {
  const DRAFT = new URLSearchParams(location.search).has('draft');
  const NARROW = matchMedia('(max-width: 640px)');
  const NARROW_SCALE = 0.68;

  // System → light → dark; apply the saved theme before first paint in <head>.
  (() => {
    const btn = document.getElementById('theme');
    const meta = document.querySelector('meta[name="theme-color"]');
    const root = document.documentElement;
    const media = matchMedia('(prefers-color-scheme: light)');
    const order = ['system', 'light', 'dark'];
    const sync = () => {
      const mode = root.dataset.theme || 'system';
      btn.querySelectorAll('svg').forEach(s => s.toggleAttribute('hidden', s.dataset.t !== mode));
      btn.title = btn.ariaLabel = 'Theme: ' + mode;
      meta.content = getComputedStyle(root).getPropertyValue('--g0').trim();
    };
    btn.addEventListener('click', () => {
      const next = order[(order.indexOf(root.dataset.theme || 'system') + 1) % order.length];
      if (next === 'system') delete root.dataset.theme; else root.dataset.theme = next;
      try { if (next === 'system') localStorage.removeItem('theme'); else localStorage.setItem('theme', next); } catch {}
      sync();
    });
    media.addEventListener('change', sync);
    sync();
  })();

  // Describe this app's capabilities without claims about other clients.
  const rows = [
    { k: 'Terminals & agent status', t: 'Keep the work in view', p: 'GPU-painted terminals, split panes and agent status alongside your worktrees. Daemon-owned sessions keep running after you close the app.', g: 'terminal' },
    { k: 'Devices', t: 'Every machine at a glance', p: 'Follow per-host load and recent agent activity across local and saved SSH machines. Coder workspaces are available on macOS and Linux; WSL devices on Windows. Other cloud integrations include experimental providers.', g: 'devices' },
    { k: 'Orchestrator', t: 'Issues to agent runs', p: 'GitHub and GitLab issues, beads and pull requests for a repository. On macOS and Linux, dispatch into an agent worktree on a supported host.', g: 'orchestrator' },
    { k: 'Fan-out', t: 'One prompt, several agents', p: 'Send a prompt to Claude, Codex, OpenCode and others in parallel lanes across supported hosts, then compare results and keep the best. Available on macOS and Linux.', g: 'fanout' },
    { k: 'Review & checkpoints', t: 'Read it, note it, rewind it', p: 'Review local checkout changes in side-by-side diffs, with line notes you can send back to the agent. Save checkpoints and restore a worktree when you want to try another direction.', g: 'review' },
    { k: 'Teleport', t: 'Move work between machines', p: 'Move a worktree, its uncommitted changes, tabs, splits and agent sessions to another supported host, after a review step. Available on macOS and Linux; custom sockets and WSL are not supported.', g: 'teleport' },
    { k: 'VS Code & browser tabs', t: 'Editors next to agents', p: 'On macOS and Windows, keep VS Code and browser tabs beside terminals in editor groups. Pin page notes and send them to the agent that opened it. Linux opens pages in the system browser.', g: 'terminal' },
  ];
  document.getElementById('rows').innerHTML = rows.map(r => `
    <div class="row">
      <div><div class="eyebrow">${r.k}</div><h3>${r.t}</h3><p>${r.p}</p></div>
      <div><div class="side-lbl"><span>Sheprd</span><span>Illustration</span></div>
        <div class="mini-gpui" aria-hidden="true"><div class="scaler" data-fit-width="780" data-fit-max="0.7"><div data-hm-app="${r.g}" data-no-sidebar></div></div></div></div>
    </div>`).join('');

  // Fixed-size illustrations fit desktops; phones can pan at a legible scale.
  function fit() {
    const stage = document.getElementById('preview'), box = document.getElementById('preview-fit');
    const narrow = NARROW.matches;
    document.body.classList.toggle('narrow', narrow);
    const s = narrow ? NARROW_SCALE : Math.min(1, box.clientWidth / 1120);
    stage.style.transform = !narrow && s < 1 ? `scale(${s})` : '';
    stage.style.zoom = narrow ? s : '';
    box.style.height = (458 * s) + 'px';
    document.querySelectorAll('[data-fit-width]').forEach(el => {
      const max = parseFloat(el.dataset.fitMax) || 1;
      const k = narrow ? NARROW_SCALE : Math.min(max, el.parentElement.clientWidth / +el.dataset.fitWidth);
      el.style.transform = narrow ? '' : `scale(${k})`;
      el.style.zoom = narrow ? k : '';
    });
  }
  window.addEventListener('resize', fit);
  fit();

  // Optional media remains hidden until loaded; ?draft shows capture briefs.
  (() => {
    const section = document.getElementById('film'), box = document.getElementById('film-box');
    const reveal = () => { section.hidden = false; document.querySelector('[data-film-link]').hidden = false; };
    const v = document.createElement('video');
    v.src = 'assets/film.mp4';
    v.poster = 'assets/film-poster.webp';
    v.controls = true; v.preload = 'metadata'; v.playsInline = true;
    v.addEventListener('loadedmetadata', () => { box.replaceChildren(v); reveal(); }, { once: true });
    v.addEventListener('error', () => {
      if (!DRAFT) return;
      box.innerHTML = '<div data-hm-video data-poster="orchestrator"></div>';
      HM.mountAll(box);
      reveal();
    }, { once: true });
  })();

  (() => {
    const order = ['teleport', 'orchestrator', 'fanout', 'review', 'checkpoints', 'devices', 'pr', 'ports', 'vscode', 'annotate', 'sidebar', 'usage', 'cells'];
    const big = new Set(['teleport', 'orchestrator']);
    const gal = document.getElementById('gal');
    const shots = order.map(id => HM.SHOTS.find(s => s.id === id)).filter(Boolean);
    const show = (t) => { t.hidden = false; document.getElementById('shots').hidden = false; document.querySelector('[data-shots-link]').hidden = false; };
    shots.forEach(s => {
      const t = document.createElement('div'), img = new Image();
      t.className = 't' + (big.has(s.id) ? ' big' : '');
      t.hidden = true;
      gal.appendChild(t);
      img.alt = s.title;
      img.loading = 'lazy';
      img.onload = () => {
        t.replaceChildren(img);
        t.insertAdjacentHTML('beforeend', `<h4>${s.title}</h4><p>${s.cap}</p>`);
        img.addEventListener('click', () => zoom(img.src, s.title, s.cap));
        show(t);
      };
      img.onerror = () => {
        if (!DRAFT) return;
        t.innerHTML = `${HM.shot(s)}<h4>${s.title}</h4><p>${s.cap}</p>`;
        show(t);
      };
      img.src = `assets/shots/${s.id}.webp`;
    });
    function zoom(src, title, cap) {
      const z = document.createElement('div');
      z.className = 'zoom';
      z.innerHTML = '<div><img alt=""><p></p></div>';
      z.querySelector('img').src = src;
      z.querySelector('img').alt = title;
      z.querySelector('p').textContent = `${title}: ${cap}`;
      const close = () => { z.remove(); document.removeEventListener('keydown', onKey); };
      const onKey = e => e.key === 'Escape' && close();
      z.addEventListener('click', close);
      document.addEventListener('keydown', onKey);
      document.body.appendChild(z);
    }
  })();

  // Cache public GitHub stars per tab; API failure leaves a plain GitHub link.
  (() => {
    const KEY = 'sheprd-stars', TTL = 10 * 60 * 1000;
    const show = n => {
      if (typeof n !== 'number' || !isFinite(n)) return;
      document.getElementById('stars-n').textContent = n >= 1000 ? (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k' : String(n);
      const el = document.getElementById('stars');
      el.hidden = false;
      el.parentElement.setAttribute('aria-label', `GitHub, ${n} stars`);
    };
    try {
      const c = JSON.parse(sessionStorage.getItem(KEY) || 'null');
      if (c && Date.now() - c.t < TTL) return show(c.n);
    } catch {}
    fetch('https://api.github.com/repos/sheprd-labs/sheprd', { headers: { Accept: 'application/vnd.github+json' } })
      .then(r => r.ok ? r.json() : null)
      .then(d => {
        if (!d || typeof d.stargazers_count !== 'number') return;
        show(d.stargazers_count);
        try { sessionStorage.setItem(KEY, JSON.stringify({ n: d.stargazers_count, t: Date.now() })); } catch {}
      }).catch(() => {});
  })();

  // All platforms remain readable without JS; with it, select the visitor's OS.
  (() => {
    const tabs = [...document.querySelectorAll('.os-tabs [role="tab"]')];
    const ua = `${navigator.userAgentData?.platform || ''} ${navigator.userAgent}`;
    const guess = /windows/i.test(ua) ? 'windows' : /linux|x11|cros/i.test(ua) && !/android/i.test(ua) ? 'linux' : 'macos';
    const select = os => tabs.forEach(t => {
      const on = t.dataset.os === os;
      t.setAttribute('aria-selected', String(on));
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute('aria-controls')).hidden = !on;
    });
    tabs.forEach((t, i) => {
      t.addEventListener('click', () => select(t.dataset.os));
      t.addEventListener('keydown', e => {
        const d = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
        if (!d) return;
        e.preventDefault();
        const next = tabs[(i + d + tabs.length) % tabs.length];
        select(next.dataset.os);
        next.focus();
      });
    });
    document.querySelector('.os-tabs').hidden = false;
    document.getElementById('install').classList.add('tabbed');
    select(guess);
  })();

  document.querySelectorAll('.cmd .copy').forEach(b => b.addEventListener('click', async () => {
    const code = b.parentElement.querySelector('code');
    const text = [...code.childNodes].filter(n => !n.classList?.contains('p')).map(n => n.textContent).join('').trim();
    try { await navigator.clipboard.writeText(text); b.textContent = 'Copied'; }
    catch { b.textContent = 'Select and copy'; }
    setTimeout(() => { b.textContent = 'Copy'; }, 1600);
  }));
})();
