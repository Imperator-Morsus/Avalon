const API_BASE = 'http://127.0.0.1:8080';

// State
let history = [];
let model = '';
let aiName = '';
let pendingPermission = null;
window.currentUser = null;
window.isAuthenticated = false;

// DOM
const chatHistory = document.getElementById('chatHistory');
const userInput = document.getElementById('userInput');
const sendBtn = document.getElementById('sendBtn');
const stopBtn = document.getElementById('stopBtn');
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
const reconnectBtn = document.getElementById('reconnectBtn');
const restartBtn = document.getElementById('restartBtn');

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
    // Build comprehensive log: chat + debug
    let md = '# Avalon Session Log\n\n';
    md += '**Generated:** ' + new Date().toISOString() + '\n\n';
    md += '---\n\n';

    // Chat history
    md += '## Chat History\n\n';
    const chatEntries = chatHistory.querySelectorAll('.message');
    if (chatEntries.length === 0) {
      md += '_No messages in this session._\n\n';
    } else {
      chatEntries.forEach(entry => {
        const role = entry.dataset.role || 'unknown';
        const text = entry.textContent.trim();
        md += `**${role}:** ${text}\n\n`;
      });
    }

    md += '---\n\n';

    // Debug log
    md += '## Debug Log\n\n';
    const debugLines = debugContent.querySelectorAll('.debug-line');
    if (debugLines.length === 0) {
      md += '_No debug entries._\n\n';
    } else {
      debugLines.forEach(line => {
        md += line.textContent.trim() + '\n\n';
      });
    }

    md += '---\n\n*End of log*\n';

    // Send to backend so it lands in logs/debug/
    const resp = await fetch('http://127.0.0.1:8080/api/debug/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content: md })
    });
    const data = await resp.json();

    const orig = statusText.textContent;
    if (data.ok && data.path) {
      statusText.textContent = 'Saved: ' + data.path;
    } else {
      statusText.textContent = 'Save failed: ' + (data.error || 'unknown');
    }
    setTimeout(() => { statusText.textContent = orig; }, 6000);
  } catch(err) {
    const orig = statusText.textContent;
    statusText.textContent = 'Save failed: ' + err.message;
    setTimeout(() => { statusText.textContent = orig; }, 4000);
  }
});

const mindmapOverlay = document.getElementById('mindmapOverlay');
const mindmapSvg = document.getElementById('mindmapSvg');
const mindmapContainer = document.getElementById('mindmapContainer');

// Graph state for Phase 4
let graphState = {
  scale: 1,
  translateX: 0,
  translateY: 0,
  isDragging: false,
  dragStartX: 0,
  dragStartY: 0,
  selectedNodeId: null,
  collapsedNodeIds: new Set(),
  theme: 'dark',
  lastData: null,
  lastSvg: null,
  lastContainer: null,
};

async function renderMindmap(data, targetSvg, targetContainer) {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = targetSvg || mindmapSvg;
  const container = targetContainer || mindmapContainer;
  const isVaultGraph = container.id === 'vaultGraphContainer';
  const canvasWrapper = container.querySelector('.graph-canvas-wrapper') || container;
  const tooltip = container.querySelector('.graph-tooltip');
  const detailPanel = container.querySelector('.graph-detail-panel');
  const sidebar = container.querySelector('.graph-sidebar');

  graphState.lastData = data;
  graphState.lastSvg = svg;
  graphState.lastContainer = container;

  svg.innerHTML = '';
  if (isVaultGraph) {
    graphState.selectedNodeId = null;
    if (detailPanel) detailPanel.classList.add('hidden');
  }

  if (!data || !data.nodes || data.nodes.length === 0) {
    const text = document.createElementNS(ns, 'text');
    text.setAttribute('x', '50%');
    text.setAttribute('y', '50%');
    text.setAttribute('text-anchor', 'middle');
    text.setAttribute('fill', 'var(--muted)');
    text.setAttribute('font-size', '14');
    text.textContent = 'No graph data available.';
    svg.appendChild(text);
    return;
  }

  const overlay = svg.closest('.mindmap-overlay');
  const wasHidden = overlay && overlay.classList.contains('hidden');
  if (wasHidden) overlay.classList.remove('hidden');

  const width = canvasWrapper.clientWidth || container.clientWidth || window.innerWidth;
  const height = canvasWrapper.clientHeight || container.clientHeight || window.innerHeight;
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);

  // Apply theme class
  container.classList.remove('graph-theme-light', 'graph-theme-dark', 'graph-theme-colorful');
  container.classList.add(`graph-theme-${graphState.theme}`);

  // Build data structures
  const nodes = data.nodes.map(n => ({ ...n, x: width / 2 + (Math.random() - 0.5) * 200, y: height / 2 + (Math.random() - 0.5) * 200 }));
  const nodeMap = new Map(nodes.map(n => [n.id, n]));
  const edges = data.edges.map(e => ({ ...e, source: nodeMap.get(e.source), target: nodeMap.get(e.target) })).filter(e => e.source && e.target);

  // Build tree structure for expand/collapse
  const childrenMap = new Map();
  for (const e of edges) {
    if (e.relation === 'contains' || e.label === 'contains') {
      if (!childrenMap.has(e.source.id)) childrenMap.set(e.source.id, []);
      childrenMap.get(e.source.id).push(e.target.id);
    }
  }
  const parentMap = new Map();
  for (const e of edges) {
    if (e.relation === 'contains' || e.label === 'contains') {
      parentMap.set(e.target.id, e.source.id);
    }
  }

  // Determine visible nodes
  const visibleNodeIds = new Set();
  const rootId = data.root;

  function addVisible(nodeId, parentVisible) {
    if (!nodeMap.has(nodeId)) return;
    if (parentVisible) visibleNodeIds.add(nodeId);
    if (graphState.collapsedNodeIds.has(nodeId)) return;
    const children = childrenMap.get(nodeId) || [];
    for (const childId of children) {
      addVisible(childId, parentVisible);
    }
  }

  addVisible(rootId, true);
  // Also add orphan nodes (no parent)
  for (const n of nodes) {
    if (!parentMap.has(n.id) && n.id !== rootId) {
      visibleNodeIds.add(n.id);
    }
  }

  const visibleNodes = nodes.filter(n => visibleNodeIds.has(n.id));
  const visibleNodeIdSet = new Set(visibleNodes.map(n => n.id));
  const visibleEdges = edges.filter(e => visibleNodeIdSet.has(e.source.id) && visibleNodeIdSet.has(e.target.id));

  // Force-directed simulation
  const k = Math.sqrt((width * height) / Math.max(visibleNodes.length, 1)) * 0.8;
  const iterations = 200;
  const chunkSize = 30;

  function runSimStep(start) {
    const end = Math.min(start + chunkSize, iterations);
    for (let i = start; i < end; i++) {
      for (let a = 0; a < visibleNodes.length; a++) {
        for (let b = a + 1; b < visibleNodes.length; b++) {
          const na = visibleNodes[a], nb = visibleNodes[b];
          let dx = na.x - nb.x, dy = na.y - nb.y;
          let dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const force = (k * k) / dist;
          const fx = (dx / dist) * force * 0.05;
          const fy = (dy / dist) * force * 0.05;
          na.x += fx; na.y += fy;
          nb.x -= fx; nb.y -= fy;
        }
      }
      for (const e of visibleEdges) {
        let dx = e.target.x - e.source.x, dy = e.target.y - e.source.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const force = (dist * dist) / k * 0.02;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        e.source.x += fx; e.source.y += fy;
        e.target.x -= fx; e.target.y -= fy;
      }
      for (const n of visibleNodes) {
        n.x += (width / 2 - n.x) * 0.01;
        n.y += (height / 2 - n.y) * 0.01;
      }
    }
    if (end < iterations) {
      requestAnimationFrame(() => runSimStep(end));
    } else {
      finishRender();
    }
  }

  function finishRender() {
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const n of visibleNodes) {
      minX = Math.min(minX, n.x); minY = Math.min(minY, n.y);
      maxX = Math.max(maxX, n.x); maxY = Math.max(maxY, n.y);
    }
    const pad = 60;
    const graphW = maxX - minX + pad * 2;
    const graphH = maxY - minY + pad * 2;
    const fitScale = Math.min(width / graphW, height / graphH, 1.2);
    const offsetX = (width - (maxX - minX) * fitScale) / 2 - minX * fitScale;
    const offsetY = (height - (maxY - minY) * fitScale) / 2 - minY * fitScale;

    // Restore previous transform if switching tabs, else fit
    if (graphState.translateX === 0 && graphState.translateY === 0 && graphState.scale === 1) {
      graphState.scale = fitScale;
      graphState.translateX = offsetX;
      graphState.translateY = offsetY;
    }
    drawScene(graphState.translateX, graphState.translateY, graphState.scale);
  }

  function drawScene(tx, ty, sc) {
    svg.innerHTML = '';
    const g = document.createElementNS(ns, 'g');
    g.setAttribute('transform', `translate(${tx},${ty}) scale(${sc})`);

    // Edges
    for (const e of visibleEdges) {
      const line = document.createElementNS(ns, 'line');
      line.setAttribute('x1', e.source.x);
      line.setAttribute('y1', e.source.y);
      line.setAttribute('x2', e.target.x);
      line.setAttribute('y2', e.target.y);
      line.setAttribute('class', 'mindmap-edge');
      g.appendChild(line);
    }

    // Nodes
    for (const n of visibleNodes) {
      const nodeG = document.createElementNS(ns, 'g');
      nodeG.setAttribute('class', `mindmap-node ${n.node_type === 'root' ? 'node-type-root' : n.node_type === 'dir' ? 'node-type-dir' : n.node_type === 'image' ? 'node-type-image' : n.node_type === 'concept' ? 'node-type-concept' : 'node-type-file'}`);
      nodeG.setAttribute('transform', `translate(${n.x},${n.y})`);
      nodeG.setAttribute('data-node-id', n.id);
      if (graphState.selectedNodeId === n.id) nodeG.classList.add('selected');

      const isRoot = n.id === rootId;
      const isDir = n.node_type === 'dir';
      const isImage = n.node_type === 'image';
      const isConcept = n.node_type === 'concept';
      const r = isRoot ? 14 : isDir ? 10 : 6;

      // Shape
      if (isDir || isRoot) {
        const rect = document.createElementNS(ns, 'rect');
        rect.setAttribute('x', -r * 1.6);
        rect.setAttribute('y', -r);
        rect.setAttribute('width', r * 3.2);
        rect.setAttribute('height', r * 2);
        rect.setAttribute('rx', r);
        rect.setAttribute('fill', isRoot ? '#e67e22' : '#5a8dee');
        nodeG.appendChild(rect);
      } else {
        const circle = document.createElementNS(ns, 'circle');
        circle.setAttribute('r', r);
        circle.setAttribute('fill', isImage ? '#c084fc' : isConcept ? '#f472b6' : '#8fd460');
        nodeG.appendChild(circle);
      }

      // Label
      const text = document.createElementNS(ns, 'text');
      text.setAttribute('text-anchor', 'middle');
      text.setAttribute('dy', isDir || isRoot ? '0.35em' : '-0.8em');
      text.textContent = n.label;
      nodeG.appendChild(text);

      // Children indicator (for collapsed nodes with children)
      const hasChildren = childrenMap.has(n.id) && (childrenMap.get(n.id) || []).length > 0;
      if (hasChildren) {
        const indicator = document.createElementNS(ns, 'circle');
        indicator.setAttribute('class', 'node-children-indicator');
        indicator.setAttribute('cx', r + 4);
        indicator.setAttribute('cy', 0);
        indicator.setAttribute('r', 3);
        indicator.setAttribute('fill', graphState.collapsedNodeIds.has(n.id) ? '#ef4444' : '#22c55e');
        indicator.style.display = hasChildren ? 'block' : 'none';
        nodeG.appendChild(indicator);
      }

      // Event handlers
      nodeG.addEventListener('click', (e) => {
        e.stopPropagation();
        if (hasChildren) {
          if (graphState.collapsedNodeIds.has(n.id)) {
            graphState.collapsedNodeIds.delete(n.id);
          } else {
            graphState.collapsedNodeIds.add(n.id);
          }
          // Re-render with same transform
          requestAnimationFrame(() => {
            renderMindmap(data, svg, container);
            graphState.translateX = tx;
            graphState.translateY = ty;
            graphState.scale = sc;
            drawScene(tx, ty, sc);
          });
        }
        if (isVaultGraph) {
          graphState.selectedNodeId = n.id;
          showNodeDetails(n);
          // Update selection highlight
          svg.querySelectorAll('.mindmap-node').forEach(el => el.classList.remove('selected'));
          nodeG.classList.add('selected');
        }
      });

      nodeG.addEventListener('mouseenter', (e) => {
        if (tooltip) {
          tooltip.textContent = `${n.label} (${n.node_type})`;
          tooltip.classList.remove('hidden');
        }
      });
      nodeG.addEventListener('mouseleave', () => {
        if (tooltip) tooltip.classList.add('hidden');
      });

      g.appendChild(nodeG);
    }

    svg.appendChild(g);

    // Update zoom label
    const zoomLabel = container.querySelector('#graphZoomLabel');
    if (zoomLabel) zoomLabel.textContent = `${Math.round(sc * 100)}%`;
  }

  // Zoom and pan handlers
  function onWheel(e) {
    e.preventDefault();
    const rect = svg.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const zoomFactor = e.deltaY < 0 ? 1.1 : 0.9;
    const newScale = Math.max(0.1, Math.min(5, graphState.scale * zoomFactor));
    const newTx = mx - (mx - graphState.translateX) * (newScale / graphState.scale);
    const newTy = my - (my - graphState.translateY) * (newScale / graphState.scale);
    graphState.scale = newScale;
    graphState.translateX = newTx;
    graphState.translateY = newTy;
    drawScene(newTx, newTy, newScale);
  }

  function onMouseDown(e) {
    if (e.target.closest('.mindmap-node')) return;
    graphState.isDragging = true;
    graphState.dragStartX = e.clientX;
    graphState.dragStartY = e.clientY;
    svg.style.cursor = 'grabbing';
  }

  function onMouseMove(e) {
    if (tooltip && !tooltip.classList.contains('hidden')) {
      const rect = container.getBoundingClientRect();
      tooltip.style.left = (e.clientX - rect.left + 12) + 'px';
      tooltip.style.top = (e.clientY - rect.top + 12) + 'px';
    }
    if (!graphState.isDragging) return;
    const dx = e.clientX - graphState.dragStartX;
    const dy = e.clientY - graphState.dragStartY;
    graphState.translateX += dx;
    graphState.translateY += dy;
    graphState.dragStartX = e.clientX;
    graphState.dragStartY = e.clientY;
    drawScene(graphState.translateX, graphState.translateY, graphState.scale);
  }

  function onMouseUp() {
    graphState.isDragging = false;
    svg.style.cursor = 'grab';
  }

  function onClickBg(e) {
    if (e.target === svg || e.target.tagName === 'g') {
      graphState.selectedNodeId = null;
      if (detailPanel) detailPanel.classList.add('hidden');
      svg.querySelectorAll('.mindmap-node').forEach(el => el.classList.remove('selected'));
    }
  }

  // Remove old listeners before adding new ones
  svg.removeEventListener('wheel', onWheel);
  svg.removeEventListener('mousedown', onMouseDown);
  svg.removeEventListener('mousemove', onMouseMove);
  svg.removeEventListener('mouseup', onMouseUp);
  svg.removeEventListener('mouseleave', onMouseUp);
  svg.removeEventListener('click', onClickBg);

  svg.addEventListener('wheel', onWheel, { passive: false });
  svg.addEventListener('mousedown', onMouseDown);
  svg.addEventListener('mousemove', onMouseMove);
  svg.addEventListener('mouseup', onMouseUp);
  svg.addEventListener('mouseleave', onMouseUp);
  svg.addEventListener('click', onClickBg);

  // Build outline tree
  if (sidebar && isVaultGraph) {
    buildOutlineTree(data, sidebar, svg, container);
  }

  // Start simulation
  runSimStep(0);
}

function buildOutlineTree(data, sidebar, svg, container) {
  const tree = sidebar.querySelector('#graphOutlineTree');
  if (!tree) return;
  tree.innerHTML = '';

  const nodeMap = new Map(data.nodes.map(n => [n.id, n]));
  const childrenMap = new Map();
  for (const e of data.edges) {
    if (e.relation === 'contains' || e.label === 'contains') {
      if (!childrenMap.has(e.source)) childrenMap.set(e.source, []);
      childrenMap.get(e.source).push(e.target);
    }
  }

  function renderNode(nodeId, depth) {
    const node = nodeMap.get(nodeId);
    if (!node) return;
    const children = childrenMap.get(nodeId) || [];
    const hasChildren = children.length > 0;
    const isCollapsed = graphState.collapsedNodeIds.has(nodeId);

    const row = document.createElement('div');
    row.className = 'graph-outline-item';
    row.style.paddingLeft = (8 + depth * 14) + 'px';

    const toggle = document.createElement('span');
    toggle.className = 'outline-toggle';
    toggle.textContent = hasChildren ? (isCollapsed ? '+' : '-') : '';
    toggle.onclick = (e) => {
      e.stopPropagation();
      if (isCollapsed) graphState.collapsedNodeIds.delete(nodeId);
      else graphState.collapsedNodeIds.add(nodeId);
      renderMindmap(data, svg, container);
    };

    const icon = document.createElement('span');
    icon.className = 'outline-icon';
    icon.textContent = node.node_type === 'root' ? '' : node.node_type === 'dir' ? '' : node.node_type === 'image' ? '' : '';

    const label = document.createElement('span');
    label.textContent = node.label;
    label.style.overflow = 'hidden';
    label.style.textOverflow = 'ellipsis';

    row.appendChild(toggle);
    row.appendChild(icon);
    row.appendChild(label);
    row.onclick = () => {
      graphState.selectedNodeId = nodeId;
      showNodeDetails(node);
      tree.querySelectorAll('.graph-outline-item').forEach(el => el.classList.remove('active'));
      row.classList.add('active');
      svg.querySelectorAll('.mindmap-node').forEach(el => el.classList.remove('selected'));
      const sel = svg.querySelector(`[data-node-id="${CSS.escape(nodeId)}"]`);
      if (sel) sel.classList.add('selected');
    };

    tree.appendChild(row);

    if (hasChildren && !isCollapsed) {
      for (const childId of children) {
        renderNode(childId, depth + 1);
      }
    }
  }

  if (data.root) renderNode(data.root, 0);
  // Orphans
  const allChildIds = new Set();
  for (const [, ids] of childrenMap) ids.forEach(id => allChildIds.add(id));
  for (const n of data.nodes) {
    if (!allChildIds.has(n.id) && n.id !== data.root) {
      renderNode(n.id, 0);
    }
  }
}

function showNodeDetails(node) {
  const panel = document.getElementById('graphDetailPanel');
  const title = document.getElementById('graphDetailTitle');
  const content = document.getElementById('graphDetailContent');
  if (!panel || !title || !content) return;

  title.textContent = node.label || 'Details';

  const meta = node.metadata || {};
  const rows = [];
  rows.push(`<div class="detail-row"><div class="detail-label">Type</div><div class="detail-value">${escapeHtml(node.node_type)}</div></div>`);
  rows.push(`<div class="detail-row"><div class="detail-label">ID</div><div class="detail-value">${node.id}</div></div>`);
  if (meta.item_id) rows.push(`<div class="detail-row"><div class="detail-label">Item ID</div><div class="detail-value">${meta.item_id}</div></div>`);
  if (meta.content_type) rows.push(`<div class="detail-row"><div class="detail-label">Content Type</div><div class="detail-value">${escapeHtml(meta.content_type)}</div></div>`);
  if (meta.warning) rows.push(`<div class="detail-row"><div class="detail-label">Warning</div><div class="detail-value" style="color:#ef4444">${escapeHtml(meta.warning)}</div></div>`);
  if (node.source_path) rows.push(`<div class="detail-row"><div class="detail-label">Source</div><div class="detail-value">${escapeHtml(node.source_path)}</div></div>`);

  content.innerHTML = rows.join('');
  panel.classList.remove('hidden');
}

// Graph toolbar handlers
function setupGraphToolbar(container) {
  const toolbar = container.querySelector('#graphToolbar');
  if (!toolbar) return;

  toolbar.querySelector('#graphZoomIn')?.addEventListener('click', () => {
    const svg = graphState.lastSvg;
    const cont = graphState.lastContainer;
    if (!svg || !cont) return;
    const newScale = Math.min(5, graphState.scale * 1.2);
    const rect = svg.getBoundingClientRect();
    const cx = rect.width / 2;
    const cy = rect.height / 2;
    graphState.translateX = cx - (cx - graphState.translateX) * (newScale / graphState.scale);
    graphState.translateY = cy - (cy - graphState.translateY) * (newScale / graphState.scale);
    graphState.scale = newScale;
    renderMindmap(graphState.lastData, svg, cont);
  });

  toolbar.querySelector('#graphZoomOut')?.addEventListener('click', () => {
    const svg = graphState.lastSvg;
    const cont = graphState.lastContainer;
    if (!svg || !cont) return;
    const newScale = Math.max(0.1, graphState.scale / 1.2);
    const rect = svg.getBoundingClientRect();
    const cx = rect.width / 2;
    const cy = rect.height / 2;
    graphState.translateX = cx - (cx - graphState.translateX) * (newScale / graphState.scale);
    graphState.translateY = cy - (cy - graphState.translateY) * (newScale / graphState.scale);
    graphState.scale = newScale;
    renderMindmap(graphState.lastData, svg, cont);
  });

  toolbar.querySelector('#graphFit')?.addEventListener('click', () => {
    graphState.scale = 1;
    graphState.translateX = 0;
    graphState.translateY = 0;
    if (graphState.lastData) {
      renderMindmap(graphState.lastData, graphState.lastSvg, graphState.lastContainer);
    }
  });

  toolbar.querySelector('#graphReset')?.addEventListener('click', () => {
    graphState.scale = 1;
    graphState.translateX = 0;
    graphState.translateY = 0;
    graphState.collapsedNodeIds.clear();
    graphState.selectedNodeId = null;
    if (graphState.lastData) {
      renderMindmap(graphState.lastData, graphState.lastSvg, graphState.lastContainer);
    }
    document.getElementById('graphDetailPanel')?.classList.add('hidden');
  });

  toolbar.querySelector('#graphOutlineToggle')?.addEventListener('click', () => {
    document.getElementById('graphSidebar')?.classList.toggle('hidden');
  });

  toolbar.querySelector('#graphThemeToggle')?.addEventListener('click', () => {
    const themes = ['dark', 'light', 'colorful'];
    const idx = themes.indexOf(graphState.theme);
    graphState.theme = themes[(idx + 1) % themes.length];
    if (graphState.lastData) {
      renderMindmap(graphState.lastData, graphState.lastSvg, graphState.lastContainer);
    }
  });

  toolbar.querySelector('#graphExportSvg')?.addEventListener('click', () => {
    const svg = graphState.lastSvg;
    if (!svg) return;
    const serializer = new XMLSerializer();
    const svgString = serializer.serializeToString(svg);
    const blob = new Blob([svgString], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'avalon_graph.svg';
    a.click();
    URL.revokeObjectURL(url);
  });

  toolbar.querySelector('#graphExportPng')?.addEventListener('click', () => {
    const svg = graphState.lastSvg;
    if (!svg) return;
    const serializer = new XMLSerializer();
    const svgString = serializer.serializeToString(svg);
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    const img = new Image();
    const svgBlob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(svgBlob);
    img.onload = () => {
      canvas.width = img.width * 2;
      canvas.height = img.height * 2;
      ctx.drawImage(img, 0, 0);
      const pngUrl = canvas.toDataURL('image/png');
      const a = document.createElement('a');
      a.href = pngUrl;
      a.download = 'avalon_graph.png';
      a.click();
      URL.revokeObjectURL(url);
    };
    img.src = url;
  });

  toolbar.querySelector('#graphExportJson')?.addEventListener('click', () => {
    if (!graphState.lastData) return;
    const json = JSON.stringify(graphState.lastData, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'avalon_graph.json';
    a.click();
    URL.revokeObjectURL(url);
  });
}

// Setup detail panel close
function setupDetailPanel() {
  document.getElementById('graphDetailClose')?.addEventListener('click', () => {
    document.getElementById('graphDetailPanel')?.classList.add('hidden');
    graphState.selectedNodeId = null;
    document.querySelectorAll('.mindmap-node').forEach(el => el.classList.remove('selected'));
  });
}

// Call setup once
setupDetailPanel();

let mindmapData = null;

// Load persisted mindmap on startup
try {
  const saved = localStorage.getItem('avalon_mindmap');
  if (saved) {
    mindmapData = JSON.parse(saved);
  }
} catch(e) {}

async function buildMindmap(show = false) {
  try {
    const res = await fetch(`${API_BASE}/api/mindmap`);
    mindmapData = await res.json();
    localStorage.setItem('avalon_mindmap', JSON.stringify(mindmapData));
    localStorage.setItem('avalon_mindmap_time', Date.now().toString());
    if (show) {
      mindmapOverlay.classList.remove('hidden');
      await renderMindmap(mindmapData);
    }
    addDebugLine(`[${ts()}] MINDMAP: ${mindmapData.nodes.length} nodes, ${mindmapData.edges.length} edges`, 'turn-end');
  } catch(err) {
    addDebugLine(`[${ts()}] MINDMAP ERROR: ${err.message}`, 'error');
  }
}

// Header Map button
document.getElementById('mapBtn').addEventListener('click', async () => {
  mindmapOverlay.classList.remove('hidden');
  if (mindmapData) {
    await renderMindmap(mindmapData);
  } else {
    await buildMindmap(true);
  }
});

document.getElementById('debugMindMapBtn').addEventListener('click', async (e) => {
  e.stopPropagation();
  await buildMindmap(true);
});

document.getElementById('mindmapCloseBtn').addEventListener('click', () => {
  mindmapOverlay.classList.add('hidden');
  // Reset to local tab on close
  currentMindmapTab = 'local';
  document.querySelectorAll('.mindmap-tab').forEach(t => t.classList.toggle('active', t.dataset.tab === 'local'));
  document.getElementById('mindmapMergeBtn').classList.add('hidden');
  document.getElementById('mindmapClearBtn').classList.add('hidden');
});

document.getElementById('mindmapResetBtn').addEventListener('click', async () => {
  if (!mindmapData) return;
  graphState.scale = 1;
  graphState.translateX = 0;
  graphState.translateY = 0;
  graphState.collapsedNodeIds.clear();
  graphState.selectedNodeId = null;
  await renderMindmap(mindmapData);
});

function applyMindmapTransform() {
  // Deprecated: renderMindmap redraws from scratch using graphState
}

function mindmapZoomAt(delta, cx, cy) {
  if (!graphState.lastData) return;
  const newScale = Math.max(0.1, Math.min(graphState.scale + delta, 5));
  if (newScale === graphState.scale) return;
  const worldX = (cx - graphState.translateX) / graphState.scale;
  const worldY = (cy - graphState.translateY) / graphState.scale;
  graphState.scale = newScale;
  graphState.translateX = cx - worldX * graphState.scale;
  graphState.translateY = cy - worldY * graphState.scale;
  renderMindmap(graphState.lastData, graphState.lastSvg, graphState.lastContainer);
}

document.getElementById('mindmapZoomInBtn').addEventListener('click', () => {
  const rect = mindmapSvg.getBoundingClientRect();
  mindmapZoomAt(0.2, rect.width / 2, rect.height / 2);
});
document.getElementById('mindmapZoomOutBtn').addEventListener('click', () => {
  const rect = mindmapSvg.getBoundingClientRect();
  mindmapZoomAt(-0.2, rect.width / 2, rect.height / 2);
});
document.getElementById('mindmapRefreshBtn').addEventListener('click', async () => {
  await buildMindmap(true);
});

// Mindmap tabs (Local / Remote)
let currentMindmapTab = 'local';

async function switchMindmapTab(tab) {
  currentMindmapTab = tab;
  document.querySelectorAll('.mindmap-tab').forEach(t => t.classList.toggle('active', t.dataset.tab === tab));
  document.getElementById('mindmapMergeBtn').classList.toggle('hidden', tab !== 'remote');
  document.getElementById('mindmapClearBtn').classList.toggle('hidden', tab !== 'remote');

  if (tab === 'local') {
    if (mindmapData) await renderMindmap(mindmapData);
  } else {
    try {
      const res = await fetch(`${API_BASE}/api/mindmap/remote`);
      const data = await res.json();
      await renderMindmap(data);
    } catch(err) {
      addDebugLine(`[${ts()}] REMOTE MINDMAP ERROR: ${err.message}`, 'error');
    }
  }
}

document.getElementById('tabLocal').addEventListener('click', () => switchMindmapTab('local'));
document.getElementById('tabRemote').addEventListener('click', () => switchMindmapTab('remote'));

document.getElementById('mindmapMergeBtn').addEventListener('click', async () => {
  try {
    const res = await fetch(`${API_BASE}/api/mindmap/merge`, { method: 'POST' });
    const data = await res.json();
    if (data.ok) {
      await buildMindmap(true);
      switchMindmapTab('local');
      statusText.textContent = 'Remote graph merged';
      setTimeout(() => statusText.textContent = 'Ready', 3000);
    } else {
      statusText.textContent = data.message || 'Merge failed';
    }
  } catch(err) {
    statusText.textContent = 'Merge failed: ' + err.message;
  }
});

document.getElementById('mindmapClearBtn').addEventListener('click', async () => {
  try {
    await fetch(`${API_BASE}/api/mindmap/remote/clear`, { method: 'POST' });
    await switchMindmapTab('remote');
    statusText.textContent = 'Remote graph cleared';
    setTimeout(() => statusText.textContent = 'Ready', 3000);
  } catch(err) {
    statusText.textContent = 'Clear failed: ' + err.message;
  }
});

// Audit panel
const auditOverlay = document.getElementById('auditOverlay');
const auditContent = document.getElementById('auditContent');

async function loadAuditSessions() {
  try {
    const res = await fetch(`${API_BASE}/api/audit/sessions`);
    const data = await res.json();
    let html = `<h3>Current Session: ${escapeHtml(data.current_session)}</h3>`;
    html += `<div style="margin: 10px 0;"><a href="${API_BASE}/api/audit/export/${encodeURIComponent(data.current_session)}" target="_blank">Export Chain of Custody (Current)</a> | <a href="${API_BASE}/api/audit/verify/${encodeURIComponent(data.current_session)}" target="_blank">Verify</a></div>`;
    if (data.sessions && data.sessions.length > 0) {
      html += '<h4>All Sessions</h4><ul>';
      data.sessions.forEach(s => {
        html += `<li>${escapeHtml(s)} — <a href="${API_BASE}/api/audit/export/${encodeURIComponent(s)}" target="_blank">Export</a> | <a href="${API_BASE}/api/audit/verify/${encodeURIComponent(s)}" target="_blank">Verify</a></li>`;
      });
      html += '</ul>';
    } else {
      html += '<p>No prior sessions found.</p>';
    }
    auditContent.innerHTML = html;
  } catch(err) {
    auditContent.innerHTML = `<p class="error">Error loading sessions: ${escapeHtml(err.message)}</p>`;
  }
}

document.getElementById('debugAuditBtn').addEventListener('click', async (e) => {
  e.stopPropagation();
  await loadAuditSessions();
  auditOverlay.classList.remove('hidden');
});

document.getElementById('auditCloseBtn').addEventListener('click', () => {
  auditOverlay.classList.add('hidden');
});

document.getElementById('auditRefreshBtn').addEventListener('click', () => {
  loadAuditSessions();
});

// Pan / zoom for mindmap is now handled internally by renderMindmap via graphState

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
  if (type === 'error' || text === 'Disconnected' || text === 'Error') {
    reconnectBtn.classList.remove('hidden');
  } else {
    reconnectBtn.classList.add('hidden');
  }
}

function setIterations(n) {
  if (n) iterCount.textContent = `Iterations: ${n}`;
  else iterCount.textContent = '';
}

// Chat helpers
function appendMessage(role, html, cls) {
  const div = document.createElement('div');
  div.className = `message ${cls || role}`;
  div.dataset.role = role;
  div.innerHTML = html;
  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
  return div;
}

function appendToolCall(tool, input) {
  const div = document.createElement('div');
  div.className = 'message tool-call';
  div.dataset.role = 'tool-call';
  div.innerHTML = `<strong>Tool:</strong> ${tool}\n<pre>${JSON.stringify(input, null, 2)}</pre>`;
  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
}

function appendToolResult(tool, result) {
  const div = document.createElement('div');
  div.className = 'message tool-result';
  div.dataset.role = 'tool-result';

  if (tool === 'analyze_video') {
    try {
      const data = typeof result === 'string' ? JSON.parse(result) : result;
      const meta = data.metadata || {};
      let html = `<strong>Video Analysis:</strong> ${escapeHtml(meta.path || '')}<br>`;
      html += `<span style="color:var(--muted);font-size:0.75rem;">`;
      html += `${meta.width || '?'}x${meta.height || '?'} | ${formatDuration(meta.duration_seconds || 0)} | ${escapeHtml(meta.codec || '?')} | ${meta.fps?.toFixed?.(2) || '?'} fps`;
      html += `</span><br>`;
      if (data.transcript) {
        html += `<details style="margin:8px 0;"><summary style="cursor:pointer;color:var(--accent);">Transcript</summary><pre style="max-height:200px;overflow-y:auto;">${escapeHtml(data.transcript.substring(0, 4000))}</pre></details>`;
      }
      if (data.frames && data.frames.length) {
        html += `<div style="display:flex;flex-wrap:wrap;gap:8px;margin-top:8px;">`;
        for (const frame of data.frames.slice(0, 10)) {
          html += `<div style="text-align:center;"><img src="data:${escapeHtml(frame.mime_type)};base64,${frame.base64}" style="max-width:120px;max-height:90px;border-radius:4px;border:1px solid var(--border);"><br><span style="font-size:0.65rem;color:var(--muted);">${formatDuration(frame.timestamp_seconds)}</span></div>`;
        }
        if (data.frames.length > 10) {
          html += `<div style="display:flex;align-items:center;justify-content:center;width:120px;height:90px;border-radius:4px;border:1px solid var(--border);color:var(--muted);font-size:0.75rem;">+${data.frames.length - 10} more</div>`;
        }
        html += `</div>`;
      }
      if (data.warnings && data.warnings.length) {
        html += `<div style="margin-top:6px;color:var(--warning);font-size:0.75rem;">${escapeHtml(data.warnings.join(', '))}</div>`;
      }
      div.innerHTML = html;
    } catch(e) {
      div.innerHTML = `<strong>${tool}:</strong>\n<pre>${escapeHtml(String(result))}</pre>`;
    }
  } else {
    div.innerHTML = `<strong>${tool}:</strong>\n<pre>${escapeHtml(String(result))}</pre>`;
  }

  chatHistory.appendChild(div);
  chatHistory.scrollTop = chatHistory.scrollHeight;
}

function formatDuration(seconds) {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
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
  const type = entry.entry_type || entry.type;
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
  sendBtn.classList.add('hidden');
  stopBtn.classList.remove('hidden');
  permPanel.classList.remove('visible');
  pendingPermission = null;

  addDebugLine(`[${ts()}] USER: ${text}`, 'turn-end');
  appendMessage('user', escapeHtml(text));
  setStatus('thinking', 'Thinking...');
  setIterations(0);

  let assistantText = '';

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
    assistantText += e.data;
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
    sendBtn.classList.add('hidden');
    stopBtn.classList.remove('hidden');
  });

  evtSource.addEventListener('error', e => {
    appendMessage('error', 'Connection error -- check that the backend is running.');
    addDebugLine(`[${ts()}] SSE ERROR: ${e.data || 'connection error'}`, 'error');
    sendBtn.classList.remove('hidden');
    stopBtn.classList.add('hidden');
    setStatus('error', 'Error');
    evtSource.close();
    evtSource = null;
  });

  evtSource.addEventListener('done', e => {
    addDebugLine(`[${ts()}] DONE -- ${e.data} iterations`, 'turn-end');
    setIterations(parseInt(e.data));
    setStatus('ready', 'Ready');
    sendBtn.classList.remove('hidden');
    stopBtn.classList.add('hidden');
    evtSource.close();
    evtSource = null;
    // Store conversation turn in history
    history.push({ role: 'user', content: text });
    history.push({ role: 'assistant', content: assistantText });
    // Cap history to last 20 turns (40 entries) to avoid token bloat
    if (history.length > 40) {
      history = history.slice(-40);
    }
  });

  evtSource.onerror = () => {
    appendMessage('error', 'SSE error -- connection lost.');
    addDebugLine(`[${ts()}] SSE ONERROR: connection lost`, 'error');
    sendBtn.classList.remove('hidden');
    stopBtn.classList.add('hidden');
    setStatus('error', 'Disconnected');
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

stopBtn.addEventListener('click', async () => {
  try {
    await fetch(`${API_BASE}/api/cancel`, { method: 'POST' });
    if (evtSource) {
      evtSource.close();
      evtSource = null;
    }
    addDebugLine(`[${ts()}] USER: Stop requested`, 'turn-end');
    appendMessage('error', 'Stopped by user.');
    setStatus('ready', 'Ready');
    sendBtn.classList.remove('hidden');
    stopBtn.classList.add('hidden');
  } catch (err) {
    addDebugLine(`[${ts()}] STOP ERROR: ${err.message}`, 'error');
  }
});

reconnectBtn.addEventListener('click', async () => {
  setStatus('thinking', 'Reconnecting...');
  try {
    const res = await fetch(`${API_BASE}/api/about`);
    if (res.ok) {
      await loadModels();
      await loadAiName();
      await loadTools();
      setStatus('ready', 'Ready');
      addDebugLine(`[${ts()}] RECONNECTED`, 'info');
    } else {
      throw new Error('Server returned ' + res.status);
    }
  } catch (e) {
    setStatus('error', 'Still disconnected');
    addDebugLine(`[${ts()}] RECONNECT FAILED: ${e.message}`, 'error');
  }
});

restartBtn.addEventListener('click', async () => {
  if (evtSource) {
    evtSource.close();
    evtSource = null;
  }
  setStatus('thinking', 'Restarting backend...');
  addDebugLine(`[${ts()}] RESTARTING BACKEND...`, 'turn-end');
  if (window.avalon && window.avalon.restartBackend) {
    window.avalon.restartBackend();
  } else {
    addDebugLine(`[${ts()}] RESTART ERROR: Electron IPC not available`, 'error');
    setStatus('error', 'Restart failed');
    return;
  }
  // Poll for backend to come back up
  let attempts = 0;
  const maxAttempts = 30;
  const interval = setInterval(async () => {
    attempts++;
    try {
      const res = await fetch(`${API_BASE}/api/about`);
      if (res.ok) {
        clearInterval(interval);
        await loadModels();
        await loadAiName();
        await loadTools();
        setStatus('ready', 'Ready');
        addDebugLine(`[${ts()}] BACKEND RESTARTED`, 'info');
      }
    } catch (e) {
      if (attempts >= maxAttempts) {
        clearInterval(interval);
        setStatus('error', 'Restart failed — backend not responding');
        addDebugLine(`[${ts()}] RESTART FAILED: backend not responding`, 'error');
      }
    }
  }, 500);
});

// ── Inline right-click spellcheck ──
const spellMenu = document.getElementById('spellMenu');
const spellCache = new Map();

function hideSpellMenu() {
  spellMenu.classList.add('hidden');
  spellMenu.innerHTML = '';
}

function getWordAtPoint(textarea, clientX, clientY) {
  const text = textarea.value;
  // If user has a selection, use it
  const selStart = textarea.selectionStart;
  const selEnd = textarea.selectionEnd;
  if (selStart !== selEnd) {
    return { word: text.substring(selStart, selEnd), start: selStart, end: selEnd };
  }

  // Mirror div technique: create an invisible element with identical styling
  // so caretPositionFromPoint works on proportional fonts accurately.
  const style = getComputedStyle(textarea);
  const mirror = document.createElement('div');
  mirror.style.position = 'fixed';
  mirror.style.top = '0';
  mirror.style.left = '0';
  mirror.style.visibility = 'hidden';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.wordWrap = 'break-word';
  mirror.style.overflowWrap = 'break-word';
  mirror.style.font = style.font;
  mirror.style.fontSize = style.fontSize;
  mirror.style.fontFamily = style.fontFamily;
  mirror.style.fontWeight = style.fontWeight;
  mirror.style.lineHeight = style.lineHeight;
  mirror.style.letterSpacing = style.letterSpacing;
  mirror.style.padding = style.padding;
  mirror.style.border = style.border;
  mirror.style.boxSizing = style.boxSizing;
  mirror.style.width = style.width;
  mirror.textContent = text + '​'; // zero-width space ensures accurate end-positioning
  document.body.appendChild(mirror);

  let offset = 0;
  if (document.caretPositionFromPoint) {
    const pos = document.caretPositionFromPoint(clientX, clientY);
    if (pos && pos.offsetNode) {
      offset = pos.offset;
    }
  } else if (document.caretRangeFromPoint) {
    const range = document.caretRangeFromPoint(clientX, clientY);
    if (range) {
      offset = range.startOffset;
    }
  }

  document.body.removeChild(mirror);

  // Clamp to valid range
  offset = Math.max(0, Math.min(text.length, offset));

  // Expand to word boundaries (letters, digits, apostrophes)
  let wStart = offset;
  let wEnd = offset;
  while (wStart > 0 && /[\w']/.test(text[wStart - 1])) wStart--;
  while (wEnd < text.length && /[\w']/.test(text[wEnd])) wEnd++;

  const word = text.substring(wStart, wEnd);
  if (!word) return null;

  return { word, start: wStart, end: wEnd };
}

userInput.addEventListener('contextmenu', async (e) => {
  // Only intercept inside textarea, and only if there's actual text
  if (!userInput.value.trim()) return;

  const wordInfo = getWordAtPoint(userInput, e.clientX, e.clientY);
  if (!wordInfo) return;

  e.preventDefault();
  hideSpellMenu();

  // Position menu
  const menuW = 200;
  const menuH = 120;
  let left = e.clientX;
  let top = e.clientY;
  if (left + menuW > window.innerWidth) left = window.innerWidth - menuW - 8;
  if (top + menuH > window.innerHeight) top = window.innerHeight - menuH - 8;
  spellMenu.style.left = left + 'px';
  spellMenu.style.top = top + 'px';

  // Show loading state
  spellMenu.innerHTML = `
    <div class="spell-menu-word">"${escapeHtml(wordInfo.word)}"</div>
    <div class="spell-menu-loading">Checking spelling...</div>
  `;
  spellMenu.classList.remove('hidden');

  // Query backend (with cache)
  const cacheKey = wordInfo.word.toLowerCase();
  let corrected = spellCache.get(cacheKey);

  if (corrected === undefined) {
    try {
      const res = await fetch(`${API_BASE}/api/spellcheck`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: wordInfo.word })
      });
      const data = await res.json();
      corrected = data.corrected || wordInfo.word;
      spellCache.set(cacheKey, corrected);
    } catch (err) {
      corrected = wordInfo.word;
    }
  }

  const isSame = corrected.trim() === wordInfo.word.trim();

  let html = `<div class="spell-menu-word">"${escapeHtml(wordInfo.word)}"</div>`;

  if (!isSame) {
    html += `<div class="spell-menu-item corrected" data-replace="${escapeHtml(corrected)}" data-start="${wordInfo.start}" data-end="${wordInfo.end}">
      <span>${escapeHtml(corrected)}</span>
    </div>`;
    html += `<div class="spell-menu-divider"></div>`;
  } else {
    html += `<div class="spell-menu-msg">No suggestions</div>`;
    html += `<div class="spell-menu-divider"></div>`;
  }

  html += `<div class="spell-menu-item" id="spellMenuIgnore">Ignore</div>`;

  spellMenu.innerHTML = html;

  // Bind replace action
  const replaceItem = spellMenu.querySelector('.spell-menu-item.corrected');
  if (replaceItem) {
    replaceItem.addEventListener('click', () => {
      const start = parseInt(replaceItem.dataset.start);
      const end = parseInt(replaceItem.dataset.end);
      const replacement = replaceItem.dataset.replace;
      const before = userInput.value.substring(0, start);
      const after = userInput.value.substring(end);
      userInput.value = before + replacement + after;
      // Place cursor after replacement
      userInput.setSelectionRange(start + replacement.length, start + replacement.length);
      hideSpellMenu();
      userInput.focus();
    });
  }

  // Bind ignore
  const ignoreItem = spellMenu.querySelector('#spellMenuIgnore');
  if (ignoreItem) {
    ignoreItem.addEventListener('click', () => {
      hideSpellMenu();
      userInput.focus();
    });
  }
});

// Hide spell menu on click elsewhere
document.addEventListener('click', (e) => {
  if (!spellMenu.contains(e.target)) hideSpellMenu();
});
document.addEventListener('scroll', hideSpellMenu, true);

// Direct fetch
const fetchBtn = document.getElementById('fetchBtn');

fetchBtn.addEventListener('click', async () => {
  const url = prompt('Enter URL to fetch:');
  if (!url || !url.trim()) return;
  fetchBtn.disabled = true;
  fetchBtn.textContent = '...';
  try {
    const res = await fetch(`${API_BASE}/api/fetch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url: url.trim() })
    });
    const data = await res.json();
    let display = '';
    if (data.error) {
      display = `<strong>Fetch Error</strong><br><pre>${escapeHtml(data.error)}</pre>`;
    } else if (data.type === 'image') {
      display = `<strong>Image fetched</strong> (${escapeHtml(data.mime_type)}, ${data.size} bytes)<br><img src="data:${escapeHtml(data.mime_type)};base64,${data.base64}" style="max-width:100%;max-height:400px;">`;
    } else if (data.type === 'pdf') {
      const text = escapeHtml(data.content?.substring(0, 2000) || '');
      display = `<strong>PDF fetched</strong> (${data.size} bytes)<br><pre>${text}</pre>`;
    } else {
      const text = escapeHtml(data.content?.substring(0, 2000) || '');
      display = `<strong>Fetched</strong> (${escapeHtml(data.type)}, ${data.size} bytes)<br><pre>${text}</pre>`;
    }
    appendMessage('user', escapeHtml(`Fetch: ${url.trim()}`));
    appendMessage('assistant', display);
    addDebugLine(`[${ts()}] DIRECT_FETCH: ${url.trim()} (${data.type || 'error'})`, data.error ? 'error' : 'info');
  } catch (err) {
    addMessage('assistant', `**Fetch Error**\n\n${err.message}`);
    addDebugLine(`[${ts()}] DIRECT_FETCH ERROR: ${err.message}`, 'error');
  } finally {
    fetchBtn.disabled = false;
    fetchBtn.innerHTML = '&#x2193;';
  }
});

// ═════════════════════════════════════════════════════════════════════════════
// Vault UI
// ═════════════════════════════════════════════════════════════════════════════

const vaultOverlay = document.getElementById('vaultOverlay');
const vaultSearchInput = document.getElementById('vaultSearchInput');
const vaultSearchBtn = document.getElementById('vaultSearchBtn');
const vaultResults = document.getElementById('vaultResults');
let currentVaultTab = 'docs';

async function searchVault(query, type) {
  try {
    let url = `${API_BASE}/api/vault/search?q=${encodeURIComponent(query)}&limit=20`;
    if (type === 'images') url += '&content_type=image';
    const res = await fetch(url);
    const data = await res.json();
    return data.results || data.items || [];
  } catch(err) {
    addDebugLine(`[${ts()}] VAULT SEARCH ERROR: ${err.message}`, 'error');
    return [];
  }
}

async function getVaultItem(id) {
  try {
    const res = await fetch(`${API_BASE}/api/vault/item/${id}`);
    return await res.json();
  } catch(err) {
    addDebugLine(`[${ts()}] VAULT READ ERROR: ${err.message}`, 'error');
    return null;
  }
}

function renderVaultResults(items, type) {
  if (!items || items.length === 0) {
    vaultResults.innerHTML = '<div style="color:var(--muted);padding:20px;text-align:center;">No results found.</div>';
    return;
  }

  if (type === 'images') {
    vaultResults.innerHTML = items.map(img => {
      let meta = {};
      try { if (img.metadata) meta = JSON.parse(img.metadata); } catch(e) {}
      return `
      <div class="vault-item" onclick="showImageDetail(${img.id})">
        <div class="vault-item-image">
          <div style="width:64px;height:64px;background:var(--panel2);border-radius:var(--radius);display:flex;align-items:center;justify-content:center;color:var(--muted);font-size:0.7rem;">${escapeHtml(img.format || 'img')}</div>
          <div>
            <div class="vault-item-title">${escapeHtml(img.source_path?.split(/[\\/]/).pop() || 'Untitled')}</div>
            <div class="vault-item-meta">${img.width || '?'}x${img.height || '?'} | ${escapeHtml(img.format || 'unknown')} | ${meta.confirmed ? 'Confirmed' : 'Unconfirmed'}</div>
            <div class="vault-item-preview">${escapeHtml(img.description || 'No description')}</div>
          </div>
        </div>
      </div>
    `}).join('');
  } else {
    vaultResults.innerHTML = items.map(doc => `
      <div class="vault-item" onclick="showDocDetail(${doc.id})">
        <div class="vault-item-title">${escapeHtml(doc.title || doc.source_path?.split(/[\\/]/).pop() || 'Untitled')}</div>
        <div class="vault-item-meta">${escapeHtml(doc.content_type || 'text')} | ${doc.size_bytes || 0} bytes | ${new Date(doc.ingested_at).toLocaleString()}</div>
        <div class="vault-item-preview">${escapeHtml((doc.content || '').substring(0, 200))}${(doc.content || '').length > 200 ? '...' : ''}</div>
      </div>
    `).join('');
  }
}

async function showDocDetail(id) {
  const doc = await getVaultItem(id);
  if (!doc) return;
  vaultResults.innerHTML = `
    <div class="vault-detail">
      <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:10px;">
        <div class="vault-item-title">${escapeHtml(doc.title || 'Untitled')}</div>
        <button class="debug-btn" onclick="runVaultSearch()">Back</button>
      </div>
      <div class="vault-item-meta">${escapeHtml(doc.content_type)} | ${doc.size_bytes} bytes | ${new Date(doc.ingested_at).toLocaleString()}</div>
      <div class="vault-item-meta">Source: ${escapeHtml(doc.source_path)}</div>
      <pre>${escapeHtml(doc.content || '')}</pre>
    </div>
  `;
}

async function showImageDetail(id) {
  const img = await getVaultItem(id);
  if (!img) return;
  let meta = {};
  try { if (img.metadata) meta = JSON.parse(img.metadata); } catch(e) {}
  vaultResults.innerHTML = `
    <div class="vault-detail">
      <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:10px;">
        <div class="vault-item-title">${escapeHtml(img.source_path?.split(/[\\/]/).pop() || 'Untitled')}</div>
        <button class="debug-btn" onclick="runVaultSearch()">Back</button>
      </div>
      <div class="vault-item-meta">${img.width || '?'}x${img.height || '?'} | ${escapeHtml(img.format || 'unknown')} | ${meta.confirmed ? 'Confirmed' : 'Unconfirmed'}</div>
      <div class="vault-item-meta">Source: ${escapeHtml(img.source_path)}</div>
      <div class="vault-item-preview" style="margin:10px 0;">${escapeHtml(img.description || 'No description')}</div>
      ${meta.tags ? `<div class="vault-item-meta">Tags: ${escapeHtml(meta.tags)}</div>` : ''}
    </div>
  `;
}

async function runVaultSearch() {
  const query = vaultSearchInput.value.trim();
  if (!query) return;
  vaultResults.innerHTML = '<div style="color:var(--muted);padding:20px;text-align:center;">Searching...</div>';
  const items = await searchVault(query, currentVaultTab);
  renderVaultResults(items, currentVaultTab);
}

vaultSearchBtn.addEventListener('click', runVaultSearch);
vaultSearchInput.addEventListener('keydown', e => {
  if (e.key === 'Enter') runVaultSearch();
});

document.getElementById('vaultBtn').addEventListener('click', () => {
  vaultOverlay.classList.remove('hidden');
  vaultSearchInput.focus();
});

document.getElementById('vaultCloseBtn').addEventListener('click', () => {
  vaultOverlay.classList.add('hidden');
});

const vaultGraphContainer = document.getElementById('vaultGraphContainer');
const vaultGraphSvg = document.getElementById('vaultGraphSvg');
const vaultSearchBar = document.getElementById('vaultSearchBar');
setupGraphToolbar(vaultGraphContainer);

async function renderVaultGraph() {
  try {
    const res = await fetch(`${API_BASE}/api/vault/mindmap`);
    const data = await res.json();
    await renderMindmap(data, vaultGraphSvg, vaultGraphContainer);
  } catch(err) {
    addDebugLine(`[${ts()}] VAULT GRAPH ERROR: ${err.message}`, 'error');
  }
}

function switchVaultTab(tab) {
  currentVaultTab = tab;
  document.querySelectorAll('#vaultOverlay .mindmap-tab').forEach(t => t.classList.toggle('active', t.dataset.tab === tab));
  vaultResults.innerHTML = '';
  vaultSearchInput.value = '';
  if (tab === 'graph') {
    vaultSearchBar.classList.add('hidden');
    vaultResults.classList.add('hidden');
    vaultGraphContainer.classList.remove('hidden');
    renderVaultGraph();
  } else {
    vaultSearchBar.classList.remove('hidden');
    vaultResults.classList.remove('hidden');
    vaultGraphContainer.classList.add('hidden');
  }
}

document.getElementById('tabVaultDocs').addEventListener('click', () => switchVaultTab('docs'));
document.getElementById('tabVaultImages').addEventListener('click', () => switchVaultTab('images'));
document.getElementById('tabVaultGraph').addEventListener('click', () => switchVaultTab('graph'));

// ═════════════════════════════════════════════════════════════════════════════
// Agent UI
// ═════════════════════════════════════════════════════════════════════════════

const agentOverlay = document.getElementById('agentOverlay');
const agentContent = document.getElementById('agentContent');

async function loadAgents() {
  try {
    const res = await fetch(`${API_BASE}/api/agents`);
    const data = await res.json();
    return data.agents || [];
  } catch(err) {
    addDebugLine(`[${ts()}] AGENT LOAD ERROR: ${err.message}`, 'error');
    return [];
  }
}

async function deleteAgent(name) {
  if (!confirm(`Delete agent "${name}"?`)) return;
  try {
    const res = await fetch(`${API_BASE}/api/agents/${encodeURIComponent(name)}`, { method: 'DELETE' });
    const data = await res.json();
    if (data.ok) {
      renderAgentPanel();
      addDebugLine(`[${ts()}] AGENT DELETED: ${name}`, 'info');
    } else {
      alert(data.error || 'Delete failed');
    }
  } catch(err) {
    alert('Delete failed: ' + err.message);
  }
}

async function createAgent() {
  const name = document.getElementById('agentNameInput').value.trim();
  const role = document.getElementById('agentRoleInput').value.trim();
  const displayName = document.getElementById('agentDisplayInput').value.trim() || null;
  const description = document.getElementById('agentDescInput').value.trim() || null;
  const systemPrompt = document.getElementById('agentPromptInput').value.trim() || null;

  const checkboxes = document.querySelectorAll('.agent-tool-checkbox:checked');
  const allowedTools = Array.from(checkboxes).map(cb => cb.dataset.tool);

  if (!name || !role) {
    alert('Name and role are required.');
    return;
  }
  if (allowedTools.length === 0) {
    alert('Select at least one allowed tool.');
    return;
  }

  try {
    const res = await fetch(`${API_BASE}/api/agents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, role, display_name: displayName, description, system_prompt: systemPrompt, allowed_tools: allowedTools })
    });
    const text = await res.text();
    const data = text ? JSON.parse(text) : {};
    if (data.id) {
      renderAgentPanel();
      addDebugLine(`[${ts()}] AGENT CREATED: ${name}`, 'info');
    } else {
      alert(data.error || 'Create failed');
    }
  } catch(err) {
    alert('Create failed: ' + err.message);
  }
}

async function renderAgentPanel() {
  const agents = await loadAgents();
  const toolsRes = await fetch(`${API_BASE}/api/tools`);
  const toolsData = await toolsRes.json();
  const allTools = (toolsData.tools || []).filter(t => t.is_core).map(t => t.name);

  let html = '<h3 style="margin-bottom:12px;">Agents</h3>';

  // Create form
  html += `
    <div class="agent-form">
      <div style="font-weight:600;margin-bottom:10px;">Create Agent</div>
      <label>Name</label>
      <input type="text" id="agentNameInput" placeholder="e.g. research_assistant" />
      <label>Display Name</label>
      <input type="text" id="agentDisplayInput" placeholder="e.g. Research Assistant" />
      <label>Role</label>
      <input type="text" id="agentRoleInput" placeholder="e.g. Code Reviewer" />
      <label>Description</label>
      <textarea id="agentDescInput" placeholder="What does this agent do?"></textarea>
      <label>System Prompt</label>
      <textarea id="agentPromptInput" placeholder="Instructions for the agent..."></textarea>
      <label>Allowed Tools</label>
      <div class="tool-checkboxes">
        ${allTools.map(t => `<label><input type="checkbox" class="agent-tool-checkbox" data-tool="${t}" /> ${t}</label>`).join('')}
      </div>
      <button class="debug-btn" style="margin-top:12px;" onclick="createAgent()">Create Agent</button>
    </div>
  `;

  // Agent list
  if (agents.length === 0) {
    html += '<div style="color:var(--muted);padding:12px;">No agents defined yet.</div>';
  } else {
    for (const agent of agents) {
      let toolsList = [];
      try { toolsList = JSON.parse(agent.allowed_tools || '[]'); } catch(e) {}
      html += `
        <div class="agent-card ${agent.is_builtin ? 'builtin' : ''}">
          <div class="agent-card-header">
            <div>
              <div class="agent-card-name">${escapeHtml(agent.display_name || agent.name)}</div>
              <div class="agent-card-role">${escapeHtml(agent.role)}</div>
            </div>
            ${agent.is_builtin ? '<span style="font-size:0.75rem;color:var(--accent);">Built-in</span>' : ''}
          </div>
          ${agent.description ? `<div style="font-size:0.8rem;color:var(--muted);margin-bottom:6px;">${escapeHtml(agent.description)}</div>` : ''}
          <div class="agent-card-tools">Tools: ${toolsList.map(t => `<span style="background:var(--panel2);padding:2px 6px;border-radius:3px;margin-right:4px;">${t}</span>`).join('')}</div>
          <div class="agent-card-actions">
            ${!agent.is_builtin ? `<button class="danger" onclick="deleteAgent('${escapeHtml(agent.name)}')">Delete</button>` : ''}
          </div>
        </div>
      `;
    }
  }

  agentContent.innerHTML = html;
}

document.getElementById('agentBtn').addEventListener('click', () => {
  agentOverlay.classList.remove('hidden');
  renderAgentPanel();
});

document.getElementById('agentCloseBtn').addEventListener('click', () => {
  agentOverlay.classList.add('hidden');
});

document.getElementById('agentRefreshBtn').addEventListener('click', () => {
  renderAgentPanel();
});

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

// Chat reset
document.getElementById('resetChatBtn').addEventListener('click', () => {
  chatHistory.innerHTML = '';
  history = [];
  window._lastDebugLog = [];
  lastDebugLen = 0;
  debugContent.innerHTML = '';
  statusText.textContent = 'Chat cleared';
  setTimeout(() => { statusText.textContent = 'Ready'; }, 2000);
});

// Window controls (custom title bar)
if (window.avalon && window.avalon.windowClose) {
  document.getElementById('winMinBtn')?.addEventListener('click', () => window.avalon.windowMinimize());
  document.getElementById('winMaxBtn')?.addEventListener('click', () => window.avalon.windowMaximize());
  document.getElementById('winCloseBtn')?.addEventListener('click', () => window.avalon.windowClose());
}

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

function renderDomainList(domains, type) {
  if (!domains || domains.length === 0) {
    return `<div class="path-list" id="${type}Domains"><div style="color:var(--muted);font-size:0.78rem;padding:4px 0;">No domains</div></div>`;
  }
  return `<div class="path-list" id="${type}Domains">` +
    domains.map((d, i) => `
      <div class="path-item">
        <code>${escapeHtml(d)}</code>
        <button class="path-remove" onclick="removeDomain('${type}', ${i})">Remove</button>
      </div>
    `).join('') +
  `</div>`;
}

window._fsConfig = { allowed_paths: [], denied_paths: [] };
window._webFetchConfig = {
  max_depth: 1,
  confirm_domains: true,
  allowed_domains: [],
  blocked_domains: [],
  timeout_secs: 10,
  max_size_mb: 5,
  respect_robots_txt: true,
  rate_limit_ms: 1000
};

async function renderSettings() {
  try {
    const [aboutRes, permsRes, fsRes, toolsRes, webRes, auditRes, secRes] = await Promise.all([
      fetch(`${API_BASE}/api/about`),
      fetch(`${API_BASE}/api/permissions`),
      fetch(`${API_BASE}/api/fs/config`),
      fetch(`${API_BASE}/api/tools`),
      fetch(`${API_BASE}/api/web/config`),
      fetch(`${API_BASE}/api/audit/config`),
      fetch(`${API_BASE}/api/security/config`),
    ]);
    const about = await aboutRes.json();
    const permsData = await permsRes.json();
    const perms = permsData.permissions || [];
    const fs = await fsRes.json();
    const toolsData = await toolsRes.json();
    const web = await webRes.json();
    const audit = await auditRes.json();
    const sec = await secRes.json();
    window._plugins = toolsData.tools || [];
    window._fsConfig = {
      default_policy: fs.default_policy || 'deny',
      allowed_paths: fs.allowed_paths || [],
      denied_paths: fs.denied_paths || [],
      max_file_size: fs.max_file_size || 10485760
    };
    window._webFetchConfig = {
      max_depth: web.max_depth ?? 1,
      confirm_domains: web.confirm_domains ?? true,
      allowed_domains: web.allowed_domains || [],
      blocked_domains: web.blocked_domains || [],
      timeout_secs: web.timeout_secs ?? 10,
      max_size_mb: web.max_size_mb ?? 5,
      respect_robots_txt: web.respect_robots_txt ?? true,
      rate_limit_ms: web.rate_limit_ms ?? 1000
    };
    window._auditConfig = {
      warm_enabled: audit.warm_enabled ?? true,
      cold_enabled: audit.cold_enabled ?? true
    };
    window._securityConfig = {
      block_private_ips: sec.block_private_ips ?? true,
      enforce_html_sanitize: sec.enforce_html_sanitize ?? true,
      require_write_permission: sec.require_write_permission ?? true,
      require_delete_permission: sec.require_delete_permission ?? true
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

    // Auth status display
    const authBanner = window.isAuthenticated && window.currentUser
      ? `<div style="background:#1a2a1a;border:1px solid #2a4a2a;border-radius:8px;padding:10px 14px;margin-bottom:16px;display:flex;align-items:center;gap:10px;">
           <span style="color:#4ade80;font-size:1rem;">&#x2705;</span>
           <span style="color:var(--text);flex:1">Logged in as <strong>${escapeHtml(window.currentUser.username)}</strong> (${window.currentUser.role})</span>
         </div>`
      : `<div style="background:#2a1a1a;border:1px solid #5a2a2a;border-radius:8px;padding:10px 14px;margin-bottom:16px;">
           <span style="color:#ef4444;">&#x26A0;</span>
           <span style="color:var(--text);">You must be logged in to change settings.</span>
         </div>`;

    settingsBody.innerHTML = `
      ${authBanner}
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

      <div class="settings-expandable" id="webFetchSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('webFetchSection')">
          <span>Web Fetch</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div class="fs-control">
            <label>Max depth</label>
            <input type="number" id="webMaxDepth" value="${window._webFetchConfig.max_depth}" min="1" max="10" step="1" onchange="updateWebFetchMaxDepth(this.value)" />
          </div>
          <div class="fs-control">
            <label>Timeout (seconds)</label>
            <input type="number" id="webTimeout" value="${window._webFetchConfig.timeout_secs}" min="1" max="120" step="1" onchange="updateWebFetchTimeout(this.value)" />
          </div>
          <div class="fs-control">
            <label>Max size (MB)</label>
            <input type="number" id="webMaxSize" value="${window._webFetchConfig.max_size_mb}" min="1" max="100" step="1" onchange="updateWebFetchMaxSize(this.value)" />
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="webConfirmDomains" ${window._webFetchConfig.confirm_domains ? 'checked' : ''} onchange="toggleWebFetchConfirm()" /> Confirm unknown domains</label>
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="webRespectRobots" ${window._webFetchConfig.respect_robots_txt ? 'checked' : ''} onchange="toggleWebFetchRobots()" /> Respect robots.txt</label>
          </div>
          <div style="font-size:0.8rem;color:var(--text);margin-bottom:4px;">Allowed domains</div>
          ${renderDomainList(window._webFetchConfig.allowed_domains, 'allowed')}
          <div class="path-add">
            <input type="text" id="allowedDomainInput" placeholder="e.g. github.com" />
            <button onclick="addDomain('allowed')">Add</button>
          </div>
          <div style="font-size:0.8rem;color:var(--text);margin:12px 0 4px;">Blocked domains</div>
          ${renderDomainList(window._webFetchConfig.blocked_domains, 'blocked')}
          <div class="path-add">
            <input type="text" id="blockedDomainInput" placeholder="e.g. evil.com" />
            <button onclick="addDomain('blocked')">Add</button>
          </div>
          <div class="fs-save-msg" id="webFetchSaveMsg"></div>
        </div>
      </div>

      <div class="settings-expandable" id="auditLogSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('auditLogSection')">
          <span>Audit Log</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div class="fs-control">
            <label><input type="checkbox" id="auditWarm" ${window._auditConfig.warm_enabled ? 'checked' : ''} onchange="toggleAuditWarm()" /> Enable warm tier archiving</label>
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="auditCold" ${window._auditConfig.cold_enabled ? 'checked' : ''} onchange="toggleAuditCold()" /> Enable cold tier archiving</label>
          </div>
          <div class="fs-save-msg" id="auditSaveMsg"></div>
        </div>
      </div>

      <div class="settings-expandable" id="securitySection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('securitySection')">
          <span>Security</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div class="fs-control">
            <label><input type="checkbox" id="secBlockPrivateIps" ${window._securityConfig.block_private_ips ? 'checked' : ''} onchange="toggleSecurity('block_private_ips')" /> Block private IP addresses (SSRF protection)</label>
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="secHtmlSanitize" ${window._securityConfig.enforce_html_sanitize ? 'checked' : ''} onchange="toggleSecurity('enforce_html_sanitize')" /> Sanitize fetched HTML (remove scripts, iframes)</label>
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="secWritePerm" ${window._securityConfig.require_write_permission ? 'checked' : ''} onchange="toggleSecurity('require_write_permission')" /> Require permission for file writes</label>
          </div>
          <div class="fs-control">
            <label><input type="checkbox" id="secDeletePerm" ${window._securityConfig.require_delete_permission ? 'checked' : ''} onchange="toggleSecurity('require_delete_permission')" /> Require permission for file deletes</label>
          </div>
          <div class="fs-save-msg" id="securitySaveMsg"></div>
        </div>
      </div>

      ${pluginsHtml}

      <div class="settings-expandable" id="vaultSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('vaultSection')">
          <span>MindVault & VisionVault</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div class="settings-row">
            <div>
              <div class="settings-label">MindVault</div>
              <div class="settings-desc">Full-text search over ingested documents, PDFs, and web scrapes</div>
            </div>
            <button class="debug-btn" onclick="vaultOverlay.classList.remove('hidden');vaultSearchInput.focus();">Open</button>
          </div>
          <div class="settings-row">
            <div>
              <div class="settings-label">VisionVault</div>
              <div class="settings-desc">Image library with searchable descriptions and tags</div>
            </div>
            <button class="debug-btn" onclick="vaultOverlay.classList.remove('hidden');switchVaultTab('images');vaultSearchInput.focus();">Open</button>
          </div>
        </div>
      </div>

      <div class="settings-expandable" id="agentSection">
        <div class="settings-expandable-header" onclick="toggleSettingsExpandable('agentSection')">
          <span>Agents</span>
          <span class="arrow">&#9654;</span>
        </div>
        <div class="settings-expandable-body">
          <div class="settings-row">
            <div>
              <div class="settings-label">Agent Management</div>
              <div class="settings-desc">Create, view, and manage AI agents with whitelisted tools</div>
            </div>
            <button class="debug-btn" onclick="agentOverlay.classList.remove('hidden');renderAgentPanel();">Open</button>
          </div>
        </div>
      </div>
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
  if (!window.isAuthenticated) {
    showLoginOverlay(); return;
  }
  try {
    const res = await authFetch(`${API_BASE}/api/fs/config`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._fsConfig)
    });
    const data = await res.json();
    if (res.status === 401) {
      // Token expired — show login
      sessionStorage.removeItem('avalon_token');
      window.currentUser = null;
      window.isAuthenticated = false;
      showLoginOverlay();
      return;
    }
    const msg = document.getElementById('fsSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('fsSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('fsSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

// Confirm settings (no-op — auth replaces this)
function confirmSettings() {}

// Lock settings (no-op — auth replaces this)
function lockSettings() {
  localStorage.setItem(CONFIRM_KEY, '0');
  renderSettings();
}

function isSettingsConfirmed() {
  return window.isAuthenticated;
}

function getConfirmRemaining() { return 0; }

// Update countdown display every second if settings panel is open (no-op now)
setInterval(() => {
  const countdown = document.getElementById('confirmCountdown');
  if (countdown && isSettingsConfirmed()) {
    const rem = getConfirmRemaining();
    countdown.textContent = rem > 0 ? rem + 's' : 'expired';
    if (rem <= 0) lockSettings();
  }
}, 1000);

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

async function updateWebFetchMaxDepth(val) {
  const num = parseInt(val, 10);
  if (isNaN(num) || num < 1 || num > 10) return;
  window._webFetchConfig.max_depth = num;
  await saveWebFetchConfig();
}

async function updateWebFetchTimeout(val) {
  const num = parseInt(val, 10);
  if (isNaN(num) || num < 1 || num > 120) return;
  window._webFetchConfig.timeout_secs = num;
  await saveWebFetchConfig();
}

async function updateWebFetchMaxSize(val) {
  const num = parseInt(val, 10);
  if (isNaN(num) || num < 1 || num > 100) return;
  window._webFetchConfig.max_size_mb = num;
  await saveWebFetchConfig();
}

async function toggleWebFetchConfirm() {
  const cb = document.getElementById('webConfirmDomains');
  if (cb) window._webFetchConfig.confirm_domains = cb.checked;
  await saveWebFetchConfig();
}

async function toggleWebFetchRobots() {
  const cb = document.getElementById('webRespectRobots');
  if (cb) window._webFetchConfig.respect_robots_txt = cb.checked;
  await saveWebFetchConfig();
}

async function addDomain(type) {
  const input = document.getElementById(type + 'DomainInput');
  if (!input) return;
  const val = input.value.trim();
  if (!val) return;
  if (type === 'allowed') {
    if (!window._webFetchConfig.allowed_domains.includes(val)) window._webFetchConfig.allowed_domains.push(val);
  } else {
    if (!window._webFetchConfig.blocked_domains.includes(val)) window._webFetchConfig.blocked_domains.push(val);
  }
  await saveWebFetchConfig();
  refreshWebFetchSection();
}

async function removeDomain(type, index) {
  if (type === 'allowed') {
    window._webFetchConfig.allowed_domains.splice(index, 1);
  } else {
    window._webFetchConfig.blocked_domains.splice(index, 1);
  }
  await saveWebFetchConfig();
  refreshWebFetchSection();
}

async function saveWebFetchConfig() {
  if (!window.isAuthenticated) { showLoginOverlay(); return; }
  try {
    const res = await authFetch(`${API_BASE}/api/web/config`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._webFetchConfig)
    });
    const data = await res.json();
    if (res.status === 401) { sessionStorage.removeItem('avalon_token'); window.isAuthenticated = false; showLoginOverlay(); return; }
    const msg = document.getElementById('webFetchSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('webFetchSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('webFetchSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._webFetchConfig)
    });
    const data = await res.json();
    if (res.status === 403) { lockSettings(); renderSettings(); return; }
    const msg = document.getElementById('webFetchSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('webFetchSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('webFetchSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

function refreshWebFetchSection() {
  const body = document.querySelector('#webFetchSection .settings-expandable-body');
  if (!body) return;
  body.innerHTML = `
    <div class="fs-control">
      <label>Max depth</label>
      <input type="number" id="webMaxDepth" value="${window._webFetchConfig.max_depth}" min="1" max="10" step="1" onchange="updateWebFetchMaxDepth(this.value)" />
    </div>
    <div class="fs-control">
      <label>Timeout (seconds)</label>
      <input type="number" id="webTimeout" value="${window._webFetchConfig.timeout_secs}" min="1" max="120" step="1" onchange="updateWebFetchTimeout(this.value)" />
    </div>
    <div class="fs-control">
      <label>Max size (MB)</label>
      <input type="number" id="webMaxSize" value="${window._webFetchConfig.max_size_mb}" min="1" max="100" step="1" onchange="updateWebFetchMaxSize(this.value)" />
    </div>
    <div class="fs-control">
      <label><input type="checkbox" id="webConfirmDomains" ${window._webFetchConfig.confirm_domains ? 'checked' : ''} onchange="toggleWebFetchConfirm()" /> Confirm unknown domains</label>
    </div>
    <div class="fs-control">
      <label><input type="checkbox" id="webRespectRobots" ${window._webFetchConfig.respect_robots_txt ? 'checked' : ''} onchange="toggleWebFetchRobots()" /> Respect robots.txt</label>
    </div>
    <div style="font-size:0.8rem;color:var(--text);margin-bottom:4px;">Allowed domains</div>
    ${renderDomainList(window._webFetchConfig.allowed_domains, 'allowed')}
    <div class="path-add">
      <input type="text" id="allowedDomainInput" placeholder="e.g. github.com" />
      <button onclick="addDomain('allowed')">Add</button>
    </div>
    <div style="font-size:0.8rem;color:var(--text);margin:12px 0 4px;">Blocked domains</div>
    ${renderDomainList(window._webFetchConfig.blocked_domains, 'blocked')}
    <div class="path-add">
      <input type="text" id="blockedDomainInput" placeholder="e.g. evil.com" />
      <button onclick="addDomain('blocked')">Add</button>
    </div>
    <div class="fs-save-msg" id="webFetchSaveMsg"></div>
  `;
}

async function saveAuditConfig() {
  if (!window.isAuthenticated) { showLoginOverlay(); return; }
  try {
    const res = await authFetch(`${API_BASE}/api/audit/config`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._auditConfig)
    });
    const data = await res.json();
    if (res.status === 401) { sessionStorage.removeItem('avalon_token'); window.isAuthenticated = false; showLoginOverlay(); return; }
    const msg = document.getElementById('auditSaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('auditSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('auditSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
    if (data.ok) setTimeout(() => { const m = document.getElementById('auditSaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('auditSaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

function toggleAuditWarm() {
  const cb = document.getElementById('auditWarm');
  window._auditConfig.warm_enabled = cb ? cb.checked : true;
  saveAuditConfig();
}

function toggleAuditCold() {
  const cb = document.getElementById('auditCold');
  window._auditConfig.cold_enabled = cb ? cb.checked : true;
  saveAuditConfig();
}

async function saveSecurityConfig() {
  if (!window.isAuthenticated) { showLoginOverlay(); return; }
  try {
    const res = await authFetch(`${API_BASE}/api/security/config`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(window._securityConfig)
    });
    const data = await res.json();
    if (res.status === 401) { sessionStorage.removeItem('avalon_token'); window.isAuthenticated = false; showLoginOverlay(); return; }
    const msg = document.getElementById('securitySaveMsg');
    if (msg) msg.textContent = data.ok ? 'Saved.' : ('Error: ' + (data.error || 'Unknown'));
    if (data.ok) setTimeout(() => { const m = document.getElementById('securitySaveMsg'); if (m) m.textContent = ''; }, 2000);
  } catch(e) {
    const msg = document.getElementById('securitySaveMsg');
    if (msg) msg.textContent = 'Save failed: ' + e.message;
  }
}

function toggleSecurity(key) {
  const map = {
    'block_private_ips': 'secBlockPrivateIps',
    'enforce_html_sanitize': 'secHtmlSanitize',
    'require_write_permission': 'secWritePerm',
    'require_delete_permission': 'secDeletePerm'
  };
  const cb = document.getElementById(map[key]);
  window._securityConfig[key] = cb ? cb.checked : true;
  saveSecurityConfig();
}

function toggleSettingsExpandable(id) {
  const el = document.getElementById(id);
  if (!el) return;
  el.classList.toggle('open');
}

// Start
checkAuthAndStart();

async function checkAuthAndStart() {
  // First check if we have a valid session
  try {
    const resp = await fetch(`${API_BASE}/api/auth/me`);
    if (resp.ok) {
      const data = await resp.json();
      if (data.ok && data.user) {
        window.currentUser = data.user;
        window.isAuthenticated = true;
        onAuthSuccess();
        return;
      }
    }
  } catch(e) {
    // Backend not ready yet
  }
  // No valid session — show login
  showLoginOverlay();
}

function onAuthSuccess() {
  document.getElementById('logoutBtn').style.display = '';
  loadModels();
  loadAiName();
  loadTools();
  setInterval(pollDebug, 100);
}

function showLoginOverlay() {
  document.getElementById('loginOverlay').classList.remove('hidden');
  document.getElementById('logoutBtn').style.display = 'none';
}

function hideLoginOverlay() {
  document.getElementById('loginOverlay').classList.add('hidden');
}

// Login form handler
document.getElementById('loginSubmit').addEventListener('click', async () => {
  const username = document.getElementById('loginUsername').value.trim();
  const password = document.getElementById('loginPassword').value;
  const errorEl = document.getElementById('loginError');
  errorEl.textContent = '';

  if (!username || !password) {
    errorEl.textContent = 'Please enter username and password';
    return;
  }

  try {
    const resp = await fetch(`${API_BASE}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password })
    });
    const data = await resp.json();
    if (data.ok && data.token) {
      sessionStorage.setItem('avalon_token', data.token);
      window.currentUser = data.user;
      window.isAuthenticated = true;
      hideLoginOverlay();
      onAuthSuccess();
    } else {
      errorEl.textContent = data.error || 'Login failed';
    }
  } catch(e) {
    errorEl.textContent = 'Connection error: ' + e.message;
  }
});

// Enter key on password field
document.getElementById('loginPassword').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') document.getElementById('loginSubmit').click();
});
document.getElementById('loginUsername').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') document.getElementById('loginPassword').focus();
});

// Logout button
document.getElementById('logoutBtn').addEventListener('click', async () => {
  const token = sessionStorage.getItem('avalon_token');
  if (token) {
    try {
      await fetch(`${API_BASE}/api/auth/logout`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': 'Bearer ' + token
        }
      });
    } catch(e) { /* ignore */ }
  }
  sessionStorage.removeItem('avalon_token');
  window.currentUser = null;
  window.isAuthenticated = false;
  showLoginOverlay();
  // Clear chat too
  chatHistory.innerHTML = '';
  history = [];
});

// authFetch wrapper — adds Authorization header if we have a token
async function authFetch(url, options = {}) {
  const token = sessionStorage.getItem('avalon_token');
  const headers = { ...(options.headers || {}) };
  if (token) headers['Authorization'] = 'Bearer ' + token;
  return fetch(url, { ...options, headers });
}
