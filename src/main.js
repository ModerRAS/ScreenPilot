// ScreenPilot frontend – communicates with the Rust backend via Tauri invoke().

const { invoke } = window.__TAURI__.core;

// ─── Application state ────────────────────────────────────────────────────────

let devices = [];
let mediaFiles = [];
let scenes = [];

// ─── Initialization ───────────────────────────────────────────────────────────

window.addEventListener('DOMContentLoaded', async () => {
  await loadMediaServer();
  await loadMediaFiles();
  await loadDevices();
  await loadScenes();

  // Auto-refresh devices every 30 seconds
  setInterval(async () => {
    await loadDevices(false);
  }, 30_000);
});

// ─── Tab switching ────────────────────────────────────────────────────────────

function showTab(tabName) {
  document.querySelectorAll('.tab-btn').forEach((btn, i) => {
    const names = ['devices', 'scenes'];
    btn.classList.toggle('active', names[i] === tabName);
  });
  document.querySelectorAll('.tab-content').forEach(el => {
    el.classList.remove('active');
  });
  document.getElementById(`tab-${tabName}`).classList.add('active');
}

// ─── Media server URL ─────────────────────────────────────────────────────────

async function loadMediaServer() {
  try {
    const url = await invoke('get_media_server_url');
    const el = document.getElementById('server-url');
    if (el) el.textContent = `Media server: ${url}`;
  } catch (e) {
    console.warn('Could not get media server URL:', e);
  }
}

// ─── Media files ──────────────────────────────────────────────────────────────

async function loadMediaFiles() {
  try {
    mediaFiles = await invoke('list_media');
  } catch (e) {
    console.warn('Could not list media files:', e);
    mediaFiles = [];
  }
}

// ─── Device discovery & rendering ────────────────────────────────────────────

async function loadDevices(triggerDiscover = false) {
  try {
    if (triggerDiscover) {
      const btn = document.getElementById('refresh-btn');
      if (btn) { btn.disabled = true; btn.innerHTML = '<span class="spinner"></span> Scanning…'; }
      devices = await invoke('discover_devices');
      if (btn) { btn.disabled = false; btn.textContent = '🔍 Discover Devices'; }
    } else {
      devices = await invoke('get_devices');
    }
    renderDevices();
    updateSceneEditorDevices();
  } catch (e) {
    showToast(`Discovery failed: ${e}`, 'error');
    const btn = document.getElementById('refresh-btn');
    if (btn) { btn.disabled = false; btn.textContent = '🔍 Discover Devices'; }
  }
}

async function refreshDevices() {
  await loadDevices(true);
}

function renderDevices() {
  const container = document.getElementById('devices-container');
  if (!devices || devices.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <p>No devices found. Click <strong>Discover Devices</strong> to scan the network.</p>
      </div>`;
    return;
  }

  container.innerHTML = devices.map(device => `
    <div class="card device-card" id="device-${sanitize(device.uuid)}">
      <div class="device-header">
        <div>
          <div class="device-name">${sanitize(device.name)}</div>
          <div class="device-ip">${sanitize(device.ip)}</div>
        </div>
        <span class="status-badge status-${device.status}">${capitalize(device.status)}</span>
      </div>

      ${device.current_media ? `
        <div class="device-current-media">
          Now playing: <span>${sanitize(device.current_media)}</span>
        </div>` : ''}

      <div class="media-selector">
        <select id="media-select-${sanitize(device.uuid)}">
          <option value="">— select media —</option>
          ${mediaFiles.map(f => `<option value="${sanitize(f)}">${sanitize(f)}</option>`).join('')}
        </select>
        <button class="btn btn-primary btn-sm"
                onclick="playOnDevice('${sanitize(device.uuid)}')">▶ Play</button>
      </div>

      <div class="device-controls">
        <button class="btn btn-secondary btn-sm"
                onclick="pauseDevice('${sanitize(device.uuid)}')"
                ${device.status !== 'playing' ? 'disabled' : ''}>⏸ Pause</button>
        <button class="btn btn-secondary btn-sm"
                onclick="stopDevice('${sanitize(device.uuid)}')"
                ${device.status === 'idle' || device.status === 'stopped' ? 'disabled' : ''}>⏹ Stop</button>
      </div>
    </div>
  `).join('');
}

// ─── Per-device playback controls ────────────────────────────────────────────

async function playOnDevice(uuid) {
  const sel = document.getElementById(`media-select-${uuid}`);
  const filename = sel ? sel.value : '';
  if (!filename) { showToast('Please select a media file first.', 'error'); return; }

  try {
    await invoke('play_on_device', { deviceUuid: uuid, mediaFilename: filename });
    showToast(`▶ Playing "${filename}" on device.`, 'success');
    devices = await invoke('get_devices');
    renderDevices();
  } catch (e) {
    showToast(`Play failed: ${e}`, 'error');
  }
}

async function pauseDevice(uuid) {
  try {
    await invoke('pause_device', { deviceUuid: uuid });
    showToast('⏸ Paused.', 'success');
    devices = await invoke('get_devices');
    renderDevices();
  } catch (e) {
    showToast(`Pause failed: ${e}`, 'error');
  }
}

async function stopDevice(uuid) {
  try {
    await invoke('stop_device', { deviceUuid: uuid });
    showToast('⏹ Stopped.', 'success');
    devices = await invoke('get_devices');
    renderDevices();
  } catch (e) {
    showToast(`Stop failed: ${e}`, 'error');
  }
}

// ─── Scenes ───────────────────────────────────────────────────────────────────

async function loadScenes() {
  try {
    scenes = await invoke('get_scenes');
    renderScenes();
  } catch (e) {
    console.warn('Could not load scenes:', e);
  }
}

function renderScenes() {
  const container = document.getElementById('scenes-container');
  if (!scenes || scenes.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <p>No scenes defined yet. Create a scene to control multiple screens at once.</p>
      </div>`;
    return;
  }

  container.innerHTML = scenes.map(scene => `
    <div class="card scene-card">
      <div class="scene-name">🎬 ${sanitize(scene.name)}</div>
      <div class="scene-assignments">
        ${Object.entries(scene.assignments).map(([uuid, file]) => {
          const dev = devices.find(d => d.uuid === uuid);
          const devName = dev ? sanitize(dev.name) : sanitize(uuid.slice(0, 8) + '…');
          return `<div class="scene-assignment-row">
            <span class="device-label">${devName}</span>
            <span>→ ${sanitize(file)}</span>
          </div>`;
        }).join('')}
      </div>
      <div class="scene-actions">
        <button class="btn btn-primary btn-sm" onclick="applyScene('${sanitize(scene.name)}')">
          ▶ Apply Scene
        </button>
        <button class="btn btn-secondary btn-sm" onclick="editScene('${sanitize(scene.name)}')">
          ✏ Edit
        </button>
        <button class="btn btn-danger btn-sm" onclick="deleteScene('${sanitize(scene.name)}')">
          🗑 Delete
        </button>
      </div>
    </div>
  `).join('');
}

async function applyScene(sceneName) {
  try {
    const results = await invoke('apply_scene', { sceneName });
    const failed = results.filter(r => !r.success);
    if (failed.length === 0) {
      showToast(`✅ Scene "${sceneName}" applied to all devices.`, 'success');
    } else {
      showToast(`⚠ Scene applied with ${failed.length} error(s).`, 'error');
    }
    devices = await invoke('get_devices');
    renderDevices();
  } catch (e) {
    showToast(`Apply scene failed: ${e}`, 'error');
  }
}

async function deleteScene(sceneName) {
  if (!confirm(`Delete scene "${sceneName}"?`)) return;
  try {
    await invoke('delete_scene', { sceneName });
    scenes = await invoke('get_scenes');
    renderScenes();
    showToast(`Scene "${sceneName}" deleted.`, 'success');
  } catch (e) {
    showToast(`Delete failed: ${e}`, 'error');
  }
}

// ─── Scene editor ─────────────────────────────────────────────────────────────

let editingSceneName = null;

function showSceneEditor(existingScene = null) {
  editingSceneName = existingScene ? existingScene.name : null;
  document.getElementById('scene-editor').classList.remove('hidden');
  document.getElementById('scene-editor-title').textContent =
    existingScene ? `Edit Scene: ${existingScene.name}` : 'New Scene';
  document.getElementById('scene-name-input').value = existingScene ? existingScene.name : '';
  updateSceneEditorDevices(existingScene);
  document.getElementById('scene-editor').scrollIntoView({ behavior: 'smooth' });
}

function hideSceneEditor() {
  document.getElementById('scene-editor').classList.add('hidden');
  editingSceneName = null;
}

function updateSceneEditorDevices(existingScene = null) {
  const container = document.getElementById('scene-assignments');
  if (!container) return;

  if (!devices || devices.length === 0) {
    container.innerHTML = `<p style="color:var(--text-dim);font-size:0.85rem;">
      No devices discovered yet. Run discovery first.</p>`;
    return;
  }

  container.innerHTML = devices.map(d => {
    const currentFile = existingScene ? (existingScene.assignments[d.uuid] || '') : '';
    return `
      <div class="assignment-row">
        <span class="assignment-device-name" title="${sanitize(d.uuid)}">
          ${sanitize(d.name)}
        </span>
        <select id="assign-${sanitize(d.uuid)}">
          <option value="">— none —</option>
          ${mediaFiles.map(f =>
            `<option value="${sanitize(f)}" ${f === currentFile ? 'selected' : ''}>${sanitize(f)}</option>`
          ).join('')}
        </select>
      </div>`;
  }).join('');
}

function editScene(sceneName) {
  const scene = scenes.find(s => s.name === sceneName);
  if (scene) showSceneEditor(scene);
  showTab('scenes');
}

async function saveScene() {
  const name = document.getElementById('scene-name-input').value.trim();
  if (!name) { showToast('Scene name is required.', 'error'); return; }

  const assignments = {};
  for (const d of devices) {
    const sel = document.getElementById(`assign-${d.uuid}`);
    if (sel && sel.value) assignments[d.uuid] = sel.value;
  }

  const scene = { name, assignments };
  try {
    await invoke('save_scene', { scene });
    scenes = await invoke('get_scenes');
    renderScenes();
    hideSceneEditor();
    showToast(`Scene "${name}" saved.`, 'success');
  } catch (e) {
    showToast(`Save failed: ${e}`, 'error');
  }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

function sanitize(str) {
  if (str == null) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function capitalize(str) {
  if (!str) return '';
  return str.charAt(0).toUpperCase() + str.slice(1);
}

let toastTimer = null;
function showToast(message, type = 'success') {
  const el = document.getElementById('toast');
  el.textContent = message;
  el.className = `toast ${type}`;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.classList.add('hidden'); }, 3500);
}
