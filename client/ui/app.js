const API_BASE = 'http://127.0.0.1:8080';

// State
let history = [];
let model = '';
let aiName = '';
let pendingPermission = null;

// DOM
const chatHistory = document.getElementById('chatHistory');
const userInput = document.getElementById('userInput');
const sendBtn = document.getElementById('sendBtn');
const debugPanel = document.getElementById('debugPanel');
const debugToggle = document.getElementById('debugToggle');
const debugContent = document.getElementById('debugContent');
const permPanel = document.getElementById('permPanel');
const permInfo = document.getElementById('permInfo');
const permApprove = document.getElementById('permApprove');
const permDeny = document.getElementById('permDeny');
const modelSelect = document.getElementById('model');
const preloadBtn = document.getElementById('preloadBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const iterCount = document.getElementById('iterCount');

// Debug panel
function ts() {
  return new Date().toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function addDebugLine(text, cls = 'info') {
  const line = document.createElement('div');
  line.className = `debug-line ${cls}`;
  line.textContent = text;
  debugContent.appendChild(line);
  debugContent.scrollTop = debugContent.scrollHeight;
}

function clearDebug() {
  debugContent.innerHTML = '';
  lastDebugLen = 0;
  window._lastDebugLog = [];
  fetch(`${API_BASE}/api/debug/clear`, { method: 'POST' }).catch(() => {});
}

debugToggle.addEventListener('click', (e) => {
  if (e.target.closest('.debug-btn')) return;
  debugPanel.classList.toggle('collapsed');
  document.querySelector('.chat-area').classList.toggle('debug-open', !debugPanel.classList.contains('collapsed'));
});

document.getElementById('debugClearBtn').addEventListener('click', (e) => {
  e.stopPropagation();
  clearDebug();
});

document.getElementById('debugSaveBtn').addEventListener('click', async (e) => {
  e.stopPropagation();
  try {
    const res = await fetch(`${API_BASE}/api/debug/save`, { method: 'POST' });
    const data = await res.json();
    if (data.ok) {
      const orig = statusText.textContent;
      statusText.textContent = 'Saved: ' + data.path;
      setTimeout(() => { statusText.textContent = orig; }, 3000);
    }
  } catch(err) {}
});

document.getElementById('debugMindMapBtn').addEventListener('click', async (e) => {
  e.stopPropagation();
  try {
    const res = await fetch(`${API_BASE}/api/mindmap`);
    const data = await res.json();
    addDebugLine(`[${ts()}] MINDMAP: ${data.nodes.length} nodes, ${data.edges.length} edges`, 'turn-end');
    addDebugLine(`  Root: ${data.root}`, 'info');
    data.nodes.slice(0, 20).forEach(n => {
      addDebugLine(`  [${n.node_type}] ${n.label}`, 'info');
    });
    if (data.nodes.length > 20) {
      addDebugLine(`  ... and ${data.nodes.length - 20} more nodes`, 'info');
    }
  } catch(err) {
    addDebugLine(`[${ts()}] MINDMAP ERROR: ${err.message}`, 'error');
  }
});

// Load models
async function loadModels() {
  try {
    const [modelsRes, modelRes] = await Promise.all([
      fetch(`${API_BASE}/api/models`),
      fetch(`${API_BASE}/api/model`),
    ]);
    const modelsData = await modelsRes.json();
    const modelData = await modelRes.json();
    modelSelect.innerHTML = '';
    (modelsData.models || []).forEach(m => {
      const opt = document.createElement('option');
      opt.value = m; opt.textContent = m;
      modelSelect.appendChild(opt);
    });
    if (modelsData.models && modelsData.models.length > 0) {
      const saved = modelData.model;
      model = saved || modelsData.models[0];
      modelSelect.value = model;
      if (!saved) model = modelsData.models[0];
      setStatus('ready', 'Ready');
    } else {
      setStatus('ready', 'No models — is Ollama running?');
    }
  } catch(e) {
    setStatus('ready', 'Error loading models: ' + e.message);
  }
}

modelSelect.addEventListener('change', async () => {
  model = modelSelect.value;
  try { await fetch(`${API_BASE}/api/model`, { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({model}) }); } catch(e) {}
});

// Load AI name
async function loadAiName() {
  try {
    const res = await fetch(`${API_BASE}/api/ai_name`);
    const data = await res.json();
    aiName = data.ai_name || 'Avalon';
  } catch(e) {
    aiName = 'Avalon';
  }
}

// Preload model
preloadBtn.addEventListener('click', async () => {
  const selected = modelSelect.value;
  if (!selected) return;
  preloadBtn.disabled = true;
  preloadBtn.classList.add('loading');
  preloadBtn.textContent = 'Loading...';
  setStatus('thinking', `Loading ${selected}...`);

  try {
    const res = await fetch(`${API_BASE}/api/preload?model=${encodeURIComponent(selected)}`);
    const data = await res.json();
    if (data.ok) {
      setStatus('ready', `${selected} ready`);
    } else {
      setStatus('ready', `Preload failed: ${data.error}`);
    }
  } catch(e) {
    setStatus('ready', `Preload error: ${e.message}`);
  } finally {
    preloadBtn.disabled = false;
    preloadBtn.classList.remove('loading');
    preloadBtn.textContent = 'Preload';
  }
});

// Status
function setStatus(type, text) {
  statusDot.className = 'dot ' + type;
  statusText.textContent = text;
}

function setIterations(n) {
  if (n) iterCount.textContent = `Iterations: ${n}`;
  else iterCount.textContent = '';
}

// Chat helpers
function appendMessage(role, html, cls) {
  const div = document.createElement('div');
  div.className = `message ${cls || role}`;
  div.innerHTML = html;
  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
  return div;
}

function appendToolCall(tool, input) {
  const div = document.createElement('div');
  div.className = 'message tool-call';
  div.innerHTML = `<strong>Tool:</strong> ${tool}\n<pre>${JSON.stringify(input, null, 2)}</pre>`;
  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
}

function appendToolResult(tool, result) {
  const div = document.createElement('div');
  div.className = 'message tool-result';
  div.innerHTML = `<strong>${tool}:</strong>\n<pre>${escapeHtml(String(result))}</pre>`;
  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
}

function escapeHtml(s) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// Poll debug log
let lastDebugLen = 0;

async function pollDebug() {
  try {
    const res = await fetch(`${API_BASE}/api/debug`);
    if (!res.ok) throw new Error('debug endpoint ' + res.status);
    const data = await res.json();
    window._lastDebugLog = data.log;
    if (data.log && data.log.length > lastDebugLen) {
      for (let i = lastDebugLen; i < data.log.length; i++) {
        renderDebugEntry(data.log[i]);
      }
      lastDebugLen = data.log.length;
    }
  } catch(e) { window._pollErr = e.message; }
}

function renderDebugEntry(entry) {
  const t = ts();
  const type = entry.type;
  const d = entry.data || {};

  if (type === 'session_start') {
    addDebugLine(`[${t}] --- SESSION START --- model: ${d.model || ''}`, 'turn-end');
  } else if (type === 'iteration_start') {
    addDebugLine(`[${t}] -- iteration ${d.iteration || '?'}`, 'info');
  } else if (type === 'api_request') {
    addDebugLine(`[${t}] LLM_REQUEST`, 'info');
  } else if (type === 'api_response') {
    addDebugLine(`[${t}] LLM_RESPONSE  (${d.elapsed_ms || '?'}ms) stop=${d.stop_reason || '?'}`, 'info');
    if (d.content && Array.isArray(d.content)) {
      d.content.forEach(block => {
        if (block.type === 'text' && block.text) {
          addDebugLine(`[${t}] RESPONSE: ${block.text.slice(0, 200)}`, 'response');
        } else if (block.type === 'tool_use') {
          addDebugLine(`[${t}] TOOL_START: ${block.name}`, 'tool-start');
          addDebugLine(`         ${JSON.stringify(block.input || {}).slice(0, 200)}`, 'tool-start');
        }
      });
    }
  } else if (type === 'api_error') {
    addDebugLine(`[${t}] ERROR: ${d.error || JSON.stringify(d)}`, 'error');
  } else if (type === 'tool_call') {
    addDebugLine(`[${t}] TOOL_START: ${d.tool || '?'}`, 'tool-start');
    if (d.input) addDebugLine(`         ${JSON.stringify(d.input).slice(0, 200)}`, 'tool-start');
  } else if (type === 'tool_result') {
    let result = d.observation || '';
    if (result.length > 300) result = result.slice(0, 300) + '...';
    addDebugLine(`[${t}] TOOL_RESULT: ${result}`, 'tool-result');
  } else if (type === 'tool_error') {
    addDebugLine(`[${t}] TOOL_ERROR: ${d.tool || ''} -- ${d.error || ''}`, 'error');
  } else if (type === 'permission_requested') {
    addDebugLine(`[${t}] PERMISSION: ${d.tool || ''} -- awaiting approval`, 'permission');
  } else if (type === 'permission_denied') {
    addDebugLine(`[${t}] PERMISSION DENIED: ${d.tool || ''}`, 'permission');
  } else if (type === 'permission_decision') {
    addDebugLine(`[${t}] PERMISSION: ${d.allowed ? 'approved' : 'denied'}`, 'permission');
  } else if (type === 'permission_revoked') {
    addDebugLine(`[${t}] PERMISSION REVOKED: ${d.tool || ''}`, 'permission');
  } else if (type === 'loop_end') {
    addDebugLine(`[${t}] --- TURN END --- ${d.iterations || '?'} iterations, stop=${d.stop_reason || '?'}`, 'turn-end');
  } else if (type === 'error') {
    addDebugLine(`[${t}] ERROR: ${d.message || JSON.stringify(d)}`, 'error');
  } else if (type === 'loop_start') {
    addDebugLine(`[${t}] --- LOOP START --- model: ${d.model || ''}`, 'turn-end');
  } else {
    addDebugLine(`[${t}] ${type}: ${JSON.stringify(d).slice(0, 200)}`, 'info');
  }
}

// SSE chat
let evtSource = null;

async function sendMessage() {
  const text = userInput.value.trim();
  if (!text) return;

  userInput.value = '';
  sendBtn.disabled = true;
  permPanel.classList.remove('visible');
  pendingPermission = null;

  addDebugLine(`[${ts()}] USER: ${text}`, 'turn-end');
  appendMessage('user', escapeHtml(text));
  setStatus('thinking', 'Thinking...');
  setIterations(0);

  if (evtSource) evtSource.close();
  const url = `${API_BASE}/api/chat?message=${encodeURIComponent(text)}&history=${encodeURIComponent(JSON.stringify(history))}&model=${encodeURIComponent(model)}`;
  evtSource = new EventSource(url);

  evtSource.addEventListener('reasoning', e => {
    addDebugLine(`[${ts()}] REASONING:`, 'thinking');
    e.data.split('\n').forEach(line => {
      if (line.trim()) addDebugLine(`    ${line}`, 'thinking');
    });
  });

  evtSource.addEventListener('text', e => {
    appendMessage('assistant', escapeHtml(e.data));
    addDebugLine(`[${ts()}] TEXT: ${e.data.slice(0, 200)}`, 'response');
  });

  evtSource.addEventListener('tool_call', e => {
    appendToolCall(e.data, {});
    addDebugLine(`[${ts()}] TOOL_CALL: ${e.data}`, 'tool-start');
  });

  evtSource.addEventListener('tool_result', e => {
    let data;
    try { data = JSON.parse(e.data); } catch { data = { observation: e.data }; }
    if (data.tool) appendToolResult(data.tool, data.observation);
    else appendMessage('assistant', escapeHtml(e.data));
    let obs = data.observation || '';
    if (obs.length > 300) obs = obs.slice(0, 300) + '...';
    addDebugLine(`[${ts()}] TOOL_RESULT: ${obs}`, 'tool-result');
  });

  evtSource.addEventListener('permission', e => {
    let raw = (e.data || '').trim();
    let data = { tool: 'unknown', input: {} };
    try {
      let parsed = JSON.parse(raw);
      data.tool = (parsed.tool || 'unknown');
      data.input = (parsed.input || {});
    } catch (err) {
      // Malformed permission event -- ignore
    }
    showPermission(data.tool, data.input);
    sendBtn.disabled = true;
  });

  evtSource.addEventListener('error', e => {
    appendMessage('error', 'Connection error -- check that the backend is running.');
    addDebugLine(`[${ts()}] SSE ERROR: ${e.data || 'connection error'}`, 'error');
    sendBtn.disabled = false;
    setStatus('ready', 'Error');
    evtSource.close();
    evtSource = null;
  });

  evtSource.addEventListener('done', e => {
    addDebugLine(`[${ts()}] DONE -- ${e.data} iterations`, 'turn-end');
    setIterations(parseInt(e.data));
    setStatus('ready', 'Ready');
    sendBtn.disabled = false;
    evtSource.close();
    evtSource = null;
  });

  evtSource.onerror = () => {
    appendMessage('error', 'SSE error -- connection lost.');
    addDebugLine(`[${ts()}] SSE ONERROR: connection lost`, 'error');
    sendBtn.disabled = false;
    setStatus('ready', 'Disconnected');
    evtSource.close();
    evtSource = null;
  };
}

// Permission
window._tools = {};

async function loadTools() {
  try {
    const res = await fetch(`${API_BASE}/api/tools`);
    const data = await res.json();
    window._tools = {};
    (data.tools || []).forEach(t => {
      window._tools[t.name] = t;
    });
  } catch(e) {
    console.error('Failed to load tools:', e);
  }
}

function showPermission(tool, input) {
  pendingPermission = { tool: tool || 'unknown', input: input || {} };
  const toolMeta = window._tools[tool];
  const desc = toolMeta ? toolMeta.description : `This tool wants to run: ${tool}`;
  permInfo.innerHTML = `
    <div class="perm-tool-name">${tool || 'unknown'}</div>
    <div class="perm-tool-desc">${desc}</div>
    <pre>${JSON.stringify(input, null, 2)}</pre>
  `;
  permPanel.classList.add('visible');
  chatHistory.scrollTop = chatHistory.scrollHeight;
}

permApprove.addEventListener('click', async () => {
  if (!pendingPermission) return;
  await fetch(`${API_BASE}/api/permission`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ tool: pendingPermission.tool, allowed: true }),
  });
  permPanel.classList.remove('visible');
  sendBtn.disabled = false;
  pendingPermission = null;
});

permDeny.addEventListener('click', async () => {
  if (!pendingPermission) return;
  await fetch(`${API_BASE}/api/permission`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ tool: pendingPermission.tool, allowed: false }),
  });
  permPanel.classList.remove('visible');
  sendBtn.disabled = false;
  pendingPermission = null;
});

// Input handling
userInput.addEventListener('keydown', e => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
});

sendBtn.addEventListener('click', sendMessage);

// Settings panel
const settingsBtn = document.getElementById('settingsBtn');
const settingsPanel = document.getElementById('settingsPanel');
const settingsCloseBtn = document.getElementById('settingsCloseBtn');
const settingsBody = document.getElementById('settingsBody');

function openSettings() {
  settingsPanel.classList.remove('hidden');
  document.querySelector('.chat-area').classList.add('settings-open');
  renderSettings();
}

function closeSettings() {
  settingsPanel.classList.add('hidden');
  document.querySelector('.chat-area').classList.remove('settings-open');
}

settingsBtn.addEventListener('click', openSettings);
settingsCloseBtn.addEventListener('click', closeSettings);

function renderPathList(paths, type) {
  if (!paths || paths.length === 0) {
    return `<div class="path-list" id="${type}Paths"><div style="color:var(--muted);font-size:0.78rem;padding:4px 0;">No paths</div></div>`;
  }
  return `<div class="path-list" id="${type}Paths">` +
    paths.map((p, i) => `
      <div class="path-item">
        <code>${escapeHtml(p)}</code>
        <button class="path-remove" onclick="removePath('${type}', ${i})">Remove</button>
      </div>
    `).join('') +
  `</div>`;
}

window._fsConfig = { allowed_paths: [], denied_paths: [] };

async function renderSettings() {
  try {
    const [aboutRes, permsRes, fsRes, toolsRes] = await Promise.all([
      fetch(`${API_BASE}/api/about`),
      fetch(`${API_BASE}/api/permissions`),
      fetch(`${API_BASE}/api/fs/config`),
      fetch(`${API_BASE}/api/tools`),
    ]);
    const about = await aboutRes.json();
    const permsData = await permsRes.json();
    const perms = permsData.permissions || [];
    const fs = await fsRes.json();
    const toolsData = await toolsRes.json();
    window._plugins = toolsData.tools || [];
    window._fsConfig = {
      default_policy: fs.default_policy || 'deny',
      allowed_paths: fs.allowed_paths || [],
      denied_paths: fs.denied_paths || [],
      max_file_size: fs.max_file_size || 10485760
    };

    let permsHtml = '';
    if (perms.length > 0) {
      permsHtml = `
        <div class="settings-section">
          <div class="settings-section-title">Active Session Permissions</div>
          ${perms.map(p => `
            <div class="settings-row">
              <div>
                <div class="settings-label">${p.tool}</div>
                <div class="settings-desc">Granted ${new Date(p.granted_at * 1000).toLocaleTimeString()}</div>
              </div>
              <button class="debug-btn" onclick="revokePermission('${p.tool}')">Revoke</button>
            </div>
          `).join('')}
        </div>
      `;
    } else {
      permsHtml = `
        <div class="settings-section">
          <div class="settings-section-title">Active Session Permissions</div>
          <div class="settings-row">
            <div class="settings-label">No active permissions</div>
          </div>
        </div>
      `;
    }

    const pluginsHtml = renderPluginsHtml();

    settingsBody.innerHTML = `
      <div class="settings-section">
        <div class="settings-section-title">Model</div>
        <div class="settings-row">
          <div>
            <div class="settings-label">Current Model</div>
            <div class="settings-desc" id="settingsCurrentModel">${model || 'none'}</div>
          </div>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-section-title">AI Assistant</div>
        <div class="settings-row">
          <div style="flex:1">
            <div class="settings-label">Name</div>
            <div class="settings-desc">What the AI calls itself in conversation</div>
            <input type="text" id="aiNameInput" value="${escapeHtml(aiName)}" placeholder="e.g. Merlin" style="margin-top:6px;width:100%;background:var(--panel2);color:var(--text);border:1px solid var(--border);border-radius:var(--radius);padding:6px 10px;font-size:0.85rem;" />
          </div>
          <button class="debug-btn" onclick="saveAiName()">Save</button>
        </div>
        <div class="fs-save-msg" id="aiNameSaveMsg"></div>
      </div>
      ${permsHtml}

      <div class="settings-expandable" id="aboutAvalonSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('aboutAvalonSection')">
          <span>About Avalon</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div style="color: var(--text); margin-bottom: 8px;">${about.title}</div>
          <div>Version: ${about.version}</div>
          <div style="margin-top: 4px;">${about.desc.replace(/\n/g, '<br>')}</div>
          <div style="margin-top: 8px;">${about.build}</div>
        </div>
      </div>

      <div class="settings-expandable" id="fsLimiterSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('fsLimiterSection')">
          <span>File System Limiter</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">

          <div class="fs-control">
            <label>Default policy</label>
            <select id="fsPolicy" onchange="updateFsPolicy(this.value)">
              <option value="deny" ${window._fsConfig.default_policy === 'deny' ? 'selected' : ''}>Deny</option>
              <option value="allow" ${window._fsConfig.default_policy === 'allow' ? 'selected' : ''}>Allow</option>
            </select>
          </div>

          <div class="fs-control">
            <label>Max file size</label>
            <input type="number" id="fsMaxSize" value="${Math.round(window._fsConfig.max_file_size / (1024 * 1024))}" min="1" step="1" onchange="updateFsMaxSize(this.value)" />
            <span style="font-size:0.75rem;color:var(--muted)">MB</span>
          </div>

          <div style="font-size:0.8rem;color:var(--text);margin-bottom:4px;">Allowed paths</div>
          ${renderPathList(window._fsConfig.allowed_paths, 'allowed')}
          <div class="path-add">
            <input type="text" id="allowedInput" placeholder="e.g. D:/Projects" />
            <button onclick="addPath('allowed')">Add</button>
          </div>

          <div style="font-size:0.8rem;color:var(--text);margin:12px 0 4px;">Denied paths</div>
          ${renderPathList(window._fsConfig.denied_paths, 'denied')}
          <div class="path-add">
            <input type="text" id="deniedInput" placeholder="e.g. D:/Secrets" />
            <button onclick="addPath('denied')">Add</button>
          </div>

          <div class="fs-save-msg" id="fsSaveMsg"></div>
        </div>
      </div>

      ${pluginsHtml}
    `;
  } catch(e) {
    settingsBody.innerHTML = `<div style="color: var(--muted); padding: 12px;">Failed to load settings.</div>`;
  }
}

function refreshFsSection() {
  const body = document.querySelector('#fsLimiterSection .settings-expandable-body');
  if (!body) return;
  body.innerHTML = `
    <div class="fs-control">
      <label>Default policy</label>
      <select id="fsPolicy" onchange="updateFsPolicy(this.value)">
        <option value="deny" ${window._fsConfig.default_policy === 'deny' ? 'selected' : ''}>Deny</option>
        <option value="allow" ${window._fsConfig.default_policy === 'allow' ? 'selected' : ''}>Allow</option>
      </select>
    </div>

    <div class="fs-control">
      <label>Max file size</label>
      <input type="number" id="fsMaxSize" value="${Math.round(window._fsConfig.max_file_size / (1024 * 1024))}" min="1" step="1" onchange="updateFsMaxSize(this.value)" />
      <span style="font-size:0.75rem;color:var(--muted)">MB</span>
    </div>

    <div style="font-size:0.8rem;color:var(--text);margin-bottom:4px;">Allowed paths</div>
    ${renderPathList(window._fsConfig.allowed_paths, 'allowed')}
    <div class="path-add">
      <input type="text" id="allowedInput" placeholder="e.g. D:/Projects" />
      <button onclick="addPath('allowed')">Add</button>
    </div>
    <div style="font-size:0.8rem;color:var(--text);margin:12px 0 4px;">Denied paths</div>
    ${renderPathList(window._fsConfig.denied_paths, 'denied')}
    <div class="path-add">
      <input type="text" id="deniedInput" placeholder="e.g. D:/Secrets" />
      <button onclick="addPath('denied')">Add</button>
    </div>
    <div class="fs-save-msg" id="fsSaveMsg"></div>
  `;
}

async function addPath(type) {
  const input = document.getElementById(type + 'Input');
  if (!input) return;
  const val = input.value.trim();
  if (!val) return;
  if (type === 'allowed') {
    if (!window._fsConfig.allowed_paths.includes(val)) window._fsConfig.allowed_paths.push(val);
  } else {
    if (!window._fsConfig.denied_paths.includes(val)) window._fsConfig.denied_paths.push(val);
  }
  await saveFsConfig();
  refreshFsSection();
}

async function removePath(type, index) {
  if (type === 'allowed') {
    window._fsConfig.allowed_paths.splice(index, 1);
  } else {
    window._fsConfig.denied_paths.splice(index, 1);
  }
  await saveFsConfig();
  refreshFsSection();
}

async function updateFsPolicy(val) {
  window._fsConfig.default_policy = val;
  await saveFsConfig();
  refreshFsSection();
}

async function updateFsMaxSize(val) {
  const num = parseInt(val, 10);
  if (isNaN(num) || num < 1) return;
  window._fsConfig.max_file_size = num * 1024 * 1024;
  await saveFsConfig();
  refreshFsSection();
}

async function saveFsConfig() {
  try {
    const res = await fetch(`${API_BASE}/api/fs/config`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._fsConfig)
    });
    const data = await res.json();
    const msg = document.getElementById('fsSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('fsSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('fsSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

async function revokePermission(tool) {
  try {
    await fetch(`${API_BASE}/api/permissions/${encodeURIComponent(tool)}`, { method: 'DELETE' });
    renderSettings();
    addDebugLine(`[${ts()}] PERMISSION REVOKED: ${tool}`, 'permission');
  } catch(e) {
    console.error('Failed to revoke permission:', e);
  }
}

function renderPluginsHtml() {
  const optional = (window._plugins || []).filter(t => !t.is_core);
  if (optional.length === 0) {
    return '';
  }
  const rows = optional.map(t => `
    <div class="plugin-row">
      <label class="plugin-checkbox-label">
        <input type="checkbox" data-plugin="${t.name}" ${t.active ? 'checked' : ''} onchange="togglePlugin('${t.name}')" />
        <span class="plugin-name">${t.name}</span>
      </label>
      <div class="plugin-desc">${t.description}</div>
    </div>
  `).join('');
  return `
    <div class="settings-expandable" id="pluginsSection">
      <div class="settings-expandable-header" onclick="toggleSettingsExpandable('pluginsSection')">
        <span>Plugins</span>
        <span class="arrow">&#9654;</span>
      </div>
      <div class="settings-expandable-body">
        <div class="plugins-list">${rows}</div>
        <div style="margin-top: 12px; display: flex; gap: 8px; align-items: center;">
          <button class="debug-btn" onclick="savePlugins()">Save Plugins</button>
          <span class="fs-save-msg" id="pluginsSaveMsg"></span>
        </div>
        <div id="pluginsRestartMsg" style="display:none; margin-top: 8px; font-size: 0.8rem; color: var(--accent);">
          Restart Avalon for changes to take full effect.
        </div>
      </div>
    </div>
  `;
}

function togglePlugin(name) {
  const p = window._plugins.find(t => t.name === name);
  if (p) p.active = !p.active;
}

async function savePlugins() {
  const activeTools = window._plugins.filter(t => t.is_core || t.active).map(t => t.name);
  try {
    const res = await fetch(`${API_BASE}/api/plugins`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ active_tools: activeTools })
    });
    const data = await res.json();
    const msg = document.getElementById('pluginsSaveMsg');
    const restart = document.getElementById('pluginsRestartMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) {
      if (restart) restart.style.display = 'block';
      setTimeout(() => { const m = document.getElementById('pluginsSaveMsg'); if (m) m.textContent = ''; }, 2000);
    }
  } catch(e) {
    const msg = document.getElementById('pluginsSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

async function saveAiName() {
  const input = document.getElementById('aiNameInput');
  if (!input) return;
  const name = input.value.trim() || 'Avalon';
  try {
    const res = await fetch(`${API_BASE}/api/ai_name`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ai_name: name })
    });
    const data = await res.json();
    aiName = name;
    const msg = document.getElementById('aiNameSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('aiNameSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('aiNameSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

function toggleSettingsExpandable(id) {
  const el = document.getElementById(id);
  if (!el) return;
  el.classList.toggle('open');
}

// Start
loadModels();
loadAiName();
loadTools();
setInterval(pollDebug, 100);
