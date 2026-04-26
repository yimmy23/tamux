(() => {
  const page = document.body.dataset.page || '';
  const isRootIndex = page === 'index.html';
  const indexHref = isRootIndex ? 'index.html' : '../index.html';
  const pageHref = (name) => isRootIndex ? `pages/${name}` : name;
  const assetHref = (name) => isRootIndex ? `assets/${name}` : `../assets/${name}`;

  const headerHtml = `
    <div class="container header-inner">
      <a class="brand" href="${indexHref}">
        <img src="${assetHref('icon.png')}" alt="tamux" class="brand-logo-img" />
        <span class="brand-text">
          <strong>tamux</strong>
          <small>daemon-first runtime</small>
        </span>
      </a>
      <button class="nav-toggle" type="button" aria-expanded="false">Menu</button>
    </div>
  `;

  const sidebarHtml = `
    <div class="sidebar-card">
      <p class="eyebrow">Docs map</p>
      <div class="nav-group">
        <p class="nav-group-title">Orientation</p>
        <ul class="side-links">
          <li><a href="${indexHref}" data-nav="index.html">Overview</a></li>
          <li><a href="${pageHref('why-tamux.html')}" data-nav="why-tamux.html">Why tamux</a></li>
          <li><a href="${pageHref('guides.html')}" data-nav="guides.html">Guides</a></li>
          <li><a href="${pageHref('guidelines.html')}" data-nav="guidelines.html">Guidelines</a></li>
          <li><a href="${pageHref('best-practices.html')}" data-nav="best-practices.html">Best Practices</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Runtime</p>
        <ul class="side-links">
          <li><a href="${pageHref('architecture.html')}" data-nav="architecture.html">Architecture</a></li>
          <li><a href="${pageHref('mission-control.html')}" data-nav="mission-control.html">Mission Control</a></li>
          <li><a href="${pageHref('workspaces.html')}" data-nav="workspaces.html">Workspaces</a></li>
          <li><a href="${pageHref('goal-runners.html')}" data-nav="goal-runners.html">Goal Runners</a></li>
          <li><a href="${pageHref('task-queue-subagents.html')}" data-nav="task-queue-subagents.html">Execution Queue</a></li>
          <li><a href="${pageHref('multi-agent.html')}" data-nav="multi-agent.html">Multi-Agent</a></li>
          <li><a href="${pageHref('threads-handoffs.html')}" data-nav="threads-handoffs.html">Threads & Handoffs</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Reliability</p>
        <ul class="side-links">
          <li><a href="${pageHref('continuity-provenance.html')}" data-nav="continuity-provenance.html">Continuity</a></li>
          <li><a href="${pageHref('liveness-recovery.html')}" data-nav="liveness-recovery.html">Liveness</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Trust & Control</p>
        <ul class="side-links">
          <li><a href="${pageHref('memory.html')}" data-nav="memory.html">Memory</a></li>
          <li><a href="${pageHref('memory-security.html')}" data-nav="memory-security.html">Memory &amp; Security</a></li>
          <li><a href="${pageHref('security.html')}" data-nav="security.html">Security</a></li>
          <li><a href="${pageHref('governance.html')}" data-nav="governance.html">Governance</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Interfaces</p>
        <ul class="side-links">
          <li><a href="${pageHref('tui.html')}" data-nav="tui.html">TUI</a></li>
          <li><a href="${pageHref('gateway-mcp.html')}" data-nav="gateway-mcp.html">Gateway & MCP</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Extension & Learning</p>
        <ul class="side-links">
          <li><a href="${pageHref('semantic-learning.html')}" data-nav="semantic-learning.html">Semantic & Learning</a></li>
          <li><a href="${pageHref('moats-intelligence.html')}" data-nav="moats-intelligence.html">Moats & Intelligence</a></li>
          <li><a href="${pageHref('plugins.html')}" data-nav="plugins.html">Plugins</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Providers</p>
        <ul class="side-links">
          <li><a href="${pageHref('providers.html')}" data-nav="providers.html">Built-in</a></li>
          <li><a href="${pageHref('custom-providers.html')}" data-nav="custom-providers.html">Custom</a></li>
        </ul>
      </div>
      <div class="nav-group">
        <p class="nav-group-title">Reference</p>
        <ul class="side-links">
          <li><a href="${pageHref('reference.html')}" data-nav="reference.html">Reference</a></li>
        </ul>
      </div>
    </div>
    <div class="sidebar-card compact">
      <p class="eyebrow">Community</p>
      <p><a href="https://discord.gg/2T2jqPfK">Join Discord</a></p>
    </div>
  `;

  const footerHtml = `
    <div class="container footer-grid">
      <div>
        <h3>tamux</h3>
        <p>Daemon-first agentic terminal multiplexer.</p>
      </div>
      <div>
        <h3>Core</h3>
        <ul>
          <li>Daemon as source of truth</li>
          <li>Workspace tasks</li>
          <li>Durable goals</li>
          <li>Memory &amp; governance</li>
        </ul>
      </div>
      <div>
        <h3>Navigate</h3>
        <ul>
          <li><a href="${pageHref('guides.html')}">Guides</a></li>
          <li><a href="${pageHref('workspaces.html')}">Workspaces</a></li>
          <li><a href="${pageHref('architecture.html')}">Architecture</a></li>
          <li><a href="${pageHref('plugins.html')}">Plugins</a></li>
        </ul>
      </div>
    </div>
  `;

  const pageShell = document.querySelector('.page-shell');
  if (pageShell && !document.querySelector('.site-header')) {
    const header = document.createElement('header');
    header.className = 'site-header';
    header.innerHTML = headerHtml;
    document.body.insertBefore(header, pageShell);
  }

  if (pageShell && !document.querySelector('.page-sidebar')) {
    const sidebar = document.createElement('aside');
    sidebar.className = 'page-sidebar';
    sidebar.innerHTML = sidebarHtml;
    pageShell.prepend(sidebar);
  }

  if (!document.querySelector('.site-footer')) {
    const footer = document.createElement('footer');
    footer.className = 'site-footer';
    footer.innerHTML = footerHtml;
    document.body.insertBefore(footer, document.currentScript);
  }

  document.querySelectorAll('[data-nav]').forEach((link) => {
    if (link.getAttribute('data-nav') === page) link.classList.add('active');
  });

  // Mobile menu toggle for sidebar navigation
  const toggle = document.querySelector('.nav-toggle');
  const sidebar = document.querySelector('.page-sidebar');
  if (toggle && sidebar) {
    toggle.addEventListener('click', () => {
      const open = sidebar.classList.toggle('open');
      toggle.setAttribute('aria-expanded', String(open));
      toggle.textContent = open ? 'Close' : 'Menu';
    });
    
    // Close menu when clicking a link (mobile)
    sidebar.querySelectorAll('a').forEach(link => {
      link.addEventListener('click', () => {
        sidebar.classList.remove('open');
        toggle.setAttribute('aria-expanded', 'false');
        toggle.textContent = 'Menu';
      });
    });
    
    // Close menu on escape key
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && sidebar.classList.contains('open')) {
        sidebar.classList.remove('open');
        toggle.setAttribute('aria-expanded', 'false');
        toggle.textContent = 'Menu';
      }
    });
  }
})();
