/**
 * ok-FPGA-Server Dashboard — Client Application
 *
 * A lightweight SPA that communicates with the management API (/mgmt/*)
 * and WebSocket (/ws/events) to display real-time server status.
 *
 * No build step, no framework — just vanilla JS.
 */

// ============================================================================
// Configuration
// ============================================================================

/** Base URL for the management API (same origin). */
const API_BASE = '/mgmt';

/** WebSocket URL for real-time events. */
const WS_URL = `ws://${window.location.host}/ws/events`;

/** How often to poll the server status (milliseconds). */
const STATUS_POLL_INTERVAL = 5000;

/** Maximum log entries to keep in the viewer. */
const MAX_LOG_ENTRIES = 500;

// ============================================================================
// State
// ============================================================================

let ws = null;
let statusPollTimer = null;
let currentPage = 'overview';

/** Cached catalog data (fetched once, used for "About" links). */
let catalogCache = null;

// ============================================================================
// Navigation
// ============================================================================

document.querySelectorAll('.nav-link').forEach(link => {
    link.addEventListener('click', (e) => {
        e.preventDefault();
        const page = link.dataset.page;
        navigateTo(page);
    });
});

/**
 * Navigate to a page by name.
 * @param {string} page - One of: overview, devices, resources, users, services, logs, settings
 */
function navigateTo(page) {
    // Update nav active state.
    document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('active'));
    const navLink = document.querySelector(`[data-page="${page}"]`);
    if (navLink) navLink.classList.add('active');

    // Show the target page, hide others.
    document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
    const pageEl = document.getElementById(`page-${page}`);
    if (pageEl) pageEl.classList.add('active');

    currentPage = page;

    // Refresh data for the page.
    switch (page) {
        case 'overview': fetchStatus(); break;
        case 'devices': fetchDevices(); break;
        case 'resources': fetchResources(); break;
        case 'users': fetchUsers(); break;
        case 'services': fetchServices(); break;
        case 'settings': fetchSettings(); break;
    }
}

// ============================================================================
// API Calls
// ============================================================================

/**
 * Fetch JSON from the management API.
 * @param {string} path - API path (e.g., '/mgmt/status')
 * @returns {Promise<any>}
 */
async function api(path) {
    try {
        const res = await fetch(path);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return await res.json();
    } catch (err) {
        console.error(`API error: ${path}`, err);
        return null;
    }
}

/**
 * POST to the management API.
 * @param {string} path - API path
 * @returns {Promise<any>}
 */
async function apiPost(path) {
    try {
        const res = await fetch(path, { method: 'POST' });
        return await res.json();
    } catch (err) {
        console.error(`API POST error: ${path}`, err);
        return null;
    }
}

// ============================================================================
// Catalog Helpers (Phase 28D)
// ============================================================================

/**
 * Look up a device "About" URL from the catalog cache.
 * Matches product_name as a prefix (e.g., "XEM8320" matches "XEM8320-AU25P").
 */
function catalogDeviceUrl(productName) {
    if (!catalogCache || !catalogCache.devices) return null;
    const entry = catalogCache.devices.find(d =>
        productName && productName.startsWith(d.product_name)
    );
    return entry ? entry.about_url : null;
}

/**
 * Look up a SYZYGY board "About" URL from the catalog cache.
 * Matches product_name, optionally refined by product_model.
 */
function catalogSyzygyUrl(productName, productModel) {
    if (!catalogCache || !catalogCache.syzygy_boards) return null;
    const entry = catalogCache.syzygy_boards.find(b => {
        if (b.product_name !== productName) return false;
        if (b.product_model && b.product_model !== productModel) return false;
        return true;
    });
    return entry ? entry.about_url : null;
}

/**
 * Look up a bitfile catalog entry (description + URL).
 */
function catalogBitfile(filename) {
    if (!catalogCache || !catalogCache.bitfiles) return null;
    return catalogCache.bitfiles.find(b => b.filename === filename) || null;
}

/**
 * Render an "About" link if URL is present, otherwise empty string.
 */
function aboutLink(url, label) {
    if (!url) return '';
    return ` <a href="${url}" target="_blank" rel="noopener" class="about-link">${label || 'About ↗'}</a>`;
}

// ============================================================================
// Status (Overview Page)
// ============================================================================

async function fetchStatus() {
    const data = await api(`${API_BASE}/status`);
    if (!data) return;

    // Update stat cards.
    setText('stat-uptime', data.uptime_human || '—');
    setText('stat-devices', data.connected_devices);
    setText('stat-resources', data.active_resources);
    setText('stat-platform', data.platform || '—');

    // Update badge.
    setText('overview-version', `v${data.version}`);

    // Update detail table.
    setText('detail-version', data.version);
    setText('detail-description', data.description || '—');
    setText('detail-fp-version', data.frontpanel_version);
    setText('detail-platform', data.platform);
    setText('detail-features', (data.compiled_features || []).join(', ') || 'none');

    // Update sidebar hostname (Phase 28D).
    if (data.hostname) {
        setText('sidebar-hostname', `Host: ${data.hostname}`);
    }

    // Fetch config for management port.
    const cfg = await api(`${API_BASE}/config`);
    if (cfg) {
        setText('detail-mgmt-port', cfg.management_port);
    }
}

/**
 * Map of product name prefixes to image filenames in /assets/images/.
 *
 * Product names from the API include an FPGA size suffix (e.g., "XEM7320-A75T")
 * that the image filenames omit (e.g., "XEM7320-A75_1.0_XEM7320-Top.jpg").
 * This map uses the base product name (without the FPGA suffix) as the key.
 */
const BOARD_IMAGES = {
    'SZG-HUB1450-AU10P-10G': 'SZG-HUB1450-AU10P-10G_1.0_SZG-HUB1450-AU10P-10G-Top.jpg',
    'XEM7305-S50':    'XEM7305-S50_1.0_XEM7305-S50-Perspective.jpg',
    'XEM7310-A200':   'XEM7310-A200_1.0_XEM7310-Top.jpg',
    'XEM7310-A75':    'XEM7310-A75_1.0_XEM7310-Top.jpg',
    'XEM7310MT-A200': 'XEM7310MT-A200_1.0_XEM7310MT-Top.jpg',
    'XEM7310MT-A75':  'XEM7310MT-A75_1.0_XEM7310MT-Top.jpg',
    'XEM7320-A200':   'XEM7320-A200_1.0_XEM7320-Top.jpg',
    'XEM7320-A75':    'XEM7320-A75_1.0_XEM7320-Top.jpg',
    'XEM7350-K160T':  'XEM7350-K160T_1.0_XEM7350-Top.jpg',
    'XEM7350-K410T-3E':'XEM7350-K410T-3E_1.0_XEM7350-Top.jpg',
    'XEM7350-K410T':  'XEM7350-K410T_1.0_XEM7350-Top.jpg',
    'XEM7350-K70T':   'XEM7350-K70T_1.0_XEM7350-Top.jpg',
    'XEM7360-K160T-3E':'XEM7360-K160T-3E_1.0_XEM7360-Perspective.jpg',
    'XEM7360-K160T':  'XEM7360-K160T_1.0_XEM7360-Perspective.jpg',
    'XEM7360-K410T-3E':'XEM7360-K410T-3E_1.0_XEM7360-Perspective.jpg',
    'XEM7360-K410T':  'XEM7360-K410T_1.0_XEM7360-Perspective.jpg',
    'XEM8310-AU25P':  'XEM8310-AU25P_1.0_XEM8310-Top.jpg',
    'XEM8320-AU25P':  'XEM8320-AU25P_1.2_XEM8320-AU25P-Top.jpg',
    'XEM8320':        'XEM8320-AU25P_1.2_XEM8320-AU25P-Top.jpg',
    'XEM8350-KU060-3E':'XEM8350-KU060-3E_1.0_XEM8350-Top.jpg',
    'XEM8350-KU060':  'XEM8350-KU060_1.0_XEM8350-Top.jpg',
    'XEM8350-KU115':  'XEM8350-KU115_1.0_XEM8350-Top.jpg',
    'XEM8370-KU11P':  'XEM8370-KU11P_1.0_XEM8370-Top.jpg',
};

/**
 * Get the board image URL for a product name.
 *
 * Tries an exact match first (e.g., "XEM7320-A75"), then progressively
 * strips trailing characters to handle FPGA size suffixes
 * (e.g., "XEM7320-A75T" → "XEM7320-A75").
 *
 * @param {string} productName - Product name from the device API
 * @returns {string|null} Image URL or null if no match
 */
function getBoardImageUrl(productName) {
    if (!productName) return null;

    // Try exact match first.
    if (BOARD_IMAGES[productName]) {
        return `/assets/images/${BOARD_IMAGES[productName]}`;
    }

    // Strip trailing FPGA suffix characters one at a time.
    // e.g., "XEM7320-A75T" → "XEM7320-A75" → match
    let name = productName;
    while (name.length > 3) {
        name = name.slice(0, -1);
        if (BOARD_IMAGES[name]) {
            return `/assets/images/${BOARD_IMAGES[name]}`;
        }
    }

    return null;
}

async function fetchDevices() {
    const devices = await api(`${API_BASE}/devices`);
    if (!devices) return;

    const tbody = document.getElementById('devices-tbody');
    const table = document.getElementById('devices-table');
    const empty = document.getElementById('devices-empty');

    if (devices.length === 0) {
        table.style.display = 'none';
        empty.style.display = 'block';
    } else {
        table.style.display = 'table';
        empty.style.display = 'none';

        tbody.innerHTML = devices.map((d, i) => {
            const imgUrl = getBoardImageUrl(d.product_name);
            const imgHtml = imgUrl
                ? `<img src="${imgUrl}" alt="${escapeHtml(d.product_name)}" class="board-thumb" title="${escapeHtml(d.product_name)}">`
                : `<span class="board-thumb-placeholder">▦</span>`;

            // Bitfile detail (shown in expanded row).
            let bitfileHtml = '<span class="text-muted">Not programmed</span>';
            if (d.is_programmed && d.bitfile) {
                bitfileHtml = `
                    <strong>${escapeHtml(d.bitfile.filename)}</strong><br>
                    <span class="text-muted">${escapeHtml(d.bitfile.description || '—')}</span><br>
                    <small>Programmed by: ${escapeHtml(d.bitfile.programmed_by)} at ${escapeHtml(d.bitfile.programmed_at)}</small>`;
            }

            // Larger image for expanded row.
            const imgLargeHtml = imgUrl
                ? `<img src="${imgUrl}" alt="${escapeHtml(d.product_name)}" class="board-img-large">`
                : '';

            return `
            <tr class="device-row" data-serial="${escapeHtml(d.serial)}" data-idx="${i}" onclick="toggleDeviceDetail(${i}, '${escapeHtml(d.serial)}')">
                <td class="td-thumb">${imgHtml}</td>
                <td><code>${escapeHtml(d.serial)}</code></td>
                <td>${escapeHtml(d.product_name)}</td>
                <td>${escapeHtml(d.interface)}</td>
                <td>${escapeHtml(d.usb_speed)}</td>
                <td><span class="badge badge-${d.state.startsWith('Programmed') ? 'success' : 'info'}">${escapeHtml(d.state)}</span></td>
            </tr>
            <tr class="device-detail-row" id="device-detail-${i}" style="display: none;">
                <td colspan="6">
                    <div class="detail-panel">
                        <div class="detail-panel-grid">
                            <div class="detail-panel-image">${imgLargeHtml}</div>
                            <div class="detail-panel-info">
                                <h4>Device Details${aboutLink(catalogDeviceUrl(d.product_name))}</h4>
                                <table class="detail-mini-table">
                                    <tr><td class="label">Serial</td><td><code>${escapeHtml(d.serial)}</code></td></tr>
                                    <tr><td class="label">Device ID</td><td>${escapeHtml(d.device_id || '—')}</td></tr>
                                    <tr><td class="label">Firmware</td><td>${d.firmware_major}.${d.firmware_minor}</td></tr>
                                    <tr><td class="label">Product ID</td><td>${d.product_id}</td></tr>
                                </table>
                                <h4 style="margin-top:12px;">Bitfile</h4>
                                <div>${bitfileHtml}</div>
                            </div>
                        </div>
                        <div class="detail-syzygy" id="syzygy-section-${i}">
                            <h4>SYZYGY Ports</h4>
                            <div id="syzygy-ports-${i}" class="text-muted">Click to load…</div>
                        </div>
                    </div>
                </td>
            </tr>`;
        }).join('');
    }
}

/**
 * Toggle visibility of a device detail row and lazy-load SYZYGY data.
 */
async function toggleDeviceDetail(idx, serial) {
    const row = document.getElementById(`device-detail-${idx}`);
    if (!row) return;

    const isVisible = row.style.display !== 'none';
    row.style.display = isVisible ? 'none' : 'table-row';

    // Lazy-load SYZYGY data on first expand.
    if (!isVisible) {
        const container = document.getElementById(`syzygy-ports-${idx}`);
        if (container && container.textContent === 'Click to load…') {
            container.textContent = 'Loading…';
            const data = await api(`${API_BASE}/devices/${serial}/syzygy`);
            if (data && data.length > 0) {
                container.innerHTML = `
                    <table class="detail-mini-table">
                        <thead><tr><th>Port</th><th>Status (mV)</th><th>Product</th><th>Model</th><th>Manufacturer</th><th></th></tr></thead>
                        <tbody>${data.map(p => {
                            const syzygyUrl = catalogSyzygyUrl(p.product_name, p.product_model);
                            return `
                            <tr>
                                <td>${p.index}</td>
                                <td>${p.port_status_mv}${p.is_populated ? ' ✓' : ''}</td>
                                <td>${escapeHtml(p.product_name || '—')}</td>
                                <td>${escapeHtml(p.product_model || '—')}</td>
                                <td>${escapeHtml(p.manufacturer_name || '—')}</td>
                                <td>${aboutLink(syzygyUrl)}</td>
                            </tr>`;
                        }).join('')}
                        </tbody>
                    </table>`;
            } else {
                container.innerHTML = '<span class="text-muted">No SYZYGY ports detected</span>';
            }
        }
    }
}

document.getElementById('btn-refresh-devices')?.addEventListener('click', fetchDevices);

// ============================================================================
// Resources Page
// ============================================================================

async function fetchResources() {
    const data = await api(`${API_BASE}/resources`);
    if (!data) return;

    const table = document.getElementById('resources-table');
    const empty = document.getElementById('resources-empty');
    const tbody = document.getElementById('resources-tbody');

    if (!data.length) {
        table.style.display = 'none';
        empty.style.display = 'block';
        return;
    }

    empty.style.display = 'none';
    table.style.display = 'table';
    tbody.innerHTML = '';

    data.forEach((res, i) => {
        const holdersStr = res.holders.length
            ? res.holders.join(', ')
            : '<span style="color: var(--text-muted)">none</span>';
        const stateBadge = res.state === 'Available'
            ? '<span class="badge badge-true">Available</span>'
            : `<span class="badge badge-warning">${escapeHtml(res.state)}</span>`;

        // Build metadata rows for detail panel.
        let metadataHtml = '<span class="text-muted">none</span>';
        if (res.metadata && Object.keys(res.metadata).length > 0) {
            metadataHtml = '<table class="detail-mini-table"><tbody>' +
                Object.entries(res.metadata).map(([k, v]) =>
                    `<tr><td class="label">${escapeHtml(k)}</td><td>${escapeHtml(v)}</td></tr>`
                ).join('') + '</tbody></table>';
        }

        tbody.innerHTML += `
            <tr class="resource-row" onclick="toggleResourceDetail(${i})">
                <td><code>${escapeHtml(res.id.substring(0, 8))}…</code></td>
                <td><strong>${escapeHtml(res.name)}</strong></td>
                <td>${escapeHtml(res.device_serial)}</td>
                <td>${stateBadge}</td>
                <td>${holdersStr}</td>
            </tr>
            <tr class="resource-detail-row" id="resource-detail-${i}" style="display: none;">
                <td colspan="5">
                    <div class="detail-panel">
                        <table class="detail-mini-table">
                            <tr><td class="label">Full ID</td><td><code>${escapeHtml(res.id)}</code></td></tr>
                            <tr><td class="label">Description</td><td>${escapeHtml(res.description || '—')}</td></tr>
                            <tr><td class="label">Access Mode</td><td>${escapeHtml(res.access_mode || '—')}</td></tr>
                            <tr><td class="label">Locked By</td><td>${escapeHtml(res.locked_by || '—')}</td></tr>
                        </table>
                        <h4 style="margin-top:12px;">Metadata</h4>
                        ${metadataHtml}
                    </div>
                </td>
            </tr>`;
    });
}

/**
 * Toggle visibility of a resource detail row.
 */
function toggleResourceDetail(idx) {
    const row = document.getElementById(`resource-detail-${idx}`);
    if (!row) return;
    row.style.display = row.style.display === 'none' ? 'table-row' : 'none';
}

// ============================================================================
// Users Page
// ============================================================================

async function fetchUsers() {
    const data = await api(`${API_BASE}/users`);
    if (!data) return;

    const table = document.getElementById('users-table');
    const empty = document.getElementById('users-empty');
    const tbody = document.getElementById('users-tbody');

    if (!data.length) {
        table.style.display = 'none';
        empty.style.display = 'block';
        return;
    }

    empty.style.display = 'none';
    table.style.display = 'table';
    tbody.innerHTML = '';

    data.forEach(user => {
        const tr = document.createElement('tr');
        const resourcesStr = user.held_resources.length
            ? user.held_resources.map(r => `<code>${r.substring(0, 8)}…</code>`).join(', ')
            : '<span style="color: var(--text-muted)">none</span>';
        const lastSeen = new Date(user.last_seen_at).toLocaleString();
        tr.innerHTML = `
            <td><strong>${user.user_id}</strong></td>
            <td>${resourcesStr}</td>
            <td>${lastSeen}</td>`;
        tbody.appendChild(tr);
    });
}

// ============================================================================
// Services Page
// ============================================================================

async function fetchServices() {
    const data = await api(`${API_BASE}/services`);
    if (!data) return;

    const tbody = document.getElementById('services-tbody');
    tbody.innerHTML = '';

    if (data.length === 0) {
        tbody.innerHTML = `
            <tr>
                <td colspan="5" style="text-align:center; color: var(--text-muted)">
                    No services registered
                </td>
            </tr>`;
        return;
    }

    data.forEach(svc => {
        const tr = document.createElement('tr');
        const statusBadge = getStatusBadge(svc.status);
        const compiledBadge = svc.compiled
            ? '<span class="badge badge-true">Yes</span>'
            : '<span class="badge badge-false">No</span>';
        const enabledBadge = svc.enabled
            ? '<span class="badge badge-true">Yes</span>'
            : '<span class="badge badge-false">No</span>';

        const isRunning = svc.status.toLowerCase().includes('running');
        const isStopped = svc.status.toLowerCase().includes('stopped');

        tr.innerHTML = `
            <td><strong>${svc.name}</strong></td>
            <td>${statusBadge}</td>
            <td>${compiledBadge}</td>
            <td>${enabledBadge}</td>
            <td>
                <button class="btn btn-success btn-sm" 
                        onclick="serviceAction('${svc.name}', 'start')"
                        ${isRunning ? 'disabled' : ''}>Start</button>
                <button class="btn btn-danger btn-sm" 
                        onclick="serviceAction('${svc.name}', 'stop')"
                        ${isStopped ? 'disabled' : ''}>Stop</button>
                <button class="btn btn-secondary btn-sm" 
                        onclick="serviceAction('${svc.name}', 'restart')">Restart</button>
            </td>`;
        tbody.appendChild(tr);
    });
}

document.getElementById('btn-refresh-services')?.addEventListener('click', fetchServices);

/**
 * Perform a service lifecycle action.
 * @param {string} name - Service name
 * @param {string} action - start, stop, or restart
 */
async function serviceAction(name, action) {
    const result = await apiPost(`${API_BASE}/services/${name}/${action}`);
    if (result) {
        addLogEntry('INFO', `Service '${name}': ${result.message}`);
    }
    // Refresh services list.
    setTimeout(fetchServices, 500);
}

// Make it accessible from onclick handlers.
window.serviceAction = serviceAction;

// ============================================================================
// Log Viewer (ADR-022)
// ============================================================================

const logContainer = document.getElementById('log-container');
const autoScrollCheckbox = document.getElementById('log-auto-scroll');

/** Current log level selected in the UI. */
let currentLogLevel = 'INFO';

// --- Log Level Selector (dropdown) ---
const logLevelSelect = document.getElementById('log-level-select');
logLevelSelect?.addEventListener('change', () => {
    currentLogLevel = logLevelSelect.value;

    // Send log level change to server via WebSocket.
    if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'set_log_level', level: currentLogLevel }));
    }

    addLogEntry('INFO', `Log level changed to ${currentLogLevel}`);
});

// --- Clear Log File ---
document.getElementById('btn-clear-logs')?.addEventListener('click', async () => {
    if (!confirm('Clear the server log file? This cannot be undone.')) return;

    try {
        const resp = await fetch('/mgmt/logs', { method: 'DELETE' });
        if (resp.ok) {
            logContainer.innerHTML = '';
            addLogEntry('INFO', 'Log file cleared');
            updateLogFileSize();
        } else {
            addLogEntry('ERROR', `Failed to clear log file: ${resp.statusText}`);
        }
    } catch (e) {
        addLogEntry('ERROR', `Failed to clear log file: ${e.message}`);
    }
});

// --- Download Log File ---
document.getElementById('btn-download-logs')?.addEventListener('click', async () => {
    try {
        const resp = await fetch('/mgmt/logs/download');
        if (!resp.ok) {
            addLogEntry('ERROR', `Download failed: ${resp.status} ${resp.statusText}`);
            return;
        }
        const blob = await resp.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'okfpga-server.log';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    } catch (e) {
        addLogEntry('ERROR', `Download failed: ${e.message}`);
    }
});

// --- Log File Size ---
async function updateLogFileSize() {
    try {
        const data = await fetchApi('/mgmt/logs/size');
        const sizeEl = document.getElementById('log-file-size');
        if (sizeEl && data.configured) {
            const kb = (data.size_bytes / 1024).toFixed(1);
            const mb = (data.size_bytes / (1024 * 1024)).toFixed(1);
            sizeEl.textContent = data.size_bytes > 1024 * 1024
                ? `(${mb} MB)` : `(${kb} KB)`;
        }
    } catch {
        // Silently ignore — non-critical.
    }
}

// Update log file size every 10 seconds and on page load.
setInterval(updateLogFileSize, 10000);

/**
 * Add a log entry to the viewer.
 * @param {string} level - Log level (INFO, WARN, ERROR, DEBUG, TRACE)
 * @param {string} message - Log message
 * @param {string} [timestamp] - Optional ISO timestamp
 */
function addLogEntry(level, message, timestamp) {
    const time = timestamp
        ? new Date(timestamp).toLocaleTimeString()
        : new Date().toLocaleTimeString();

    const levelClass = `log-${level.toLowerCase()}`;

    const entry = document.createElement('div');
    entry.className = `log-entry ${levelClass}`;
    entry.innerHTML = `
        <span class="log-time">${time}</span>
        <span class="log-level">${level.toUpperCase().padEnd(5)}</span>
        <span class="log-msg">${escapeHtml(message)}</span>`;

    logContainer.appendChild(entry);

    // Trim old entries.
    while (logContainer.children.length > MAX_LOG_ENTRIES) {
        logContainer.removeChild(logContainer.firstChild);
    }

    // Auto-scroll.
    if (autoScrollCheckbox?.checked) {
        logContainer.scrollTop = logContainer.scrollHeight;
    }
}

// ============================================================================
// WebSocket Connection
// ============================================================================

function connectWebSocket() {
    const statusEl = document.getElementById('ws-status');
    const dot = statusEl.querySelector('.status-dot');
    const text = statusEl.querySelector('.status-text');

    try {
        ws = new WebSocket(WS_URL);
    } catch (e) {
        dot.className = 'status-dot disconnected';
        text.textContent = 'WS unavailable';
        return;
    }

    ws.onopen = () => {
        dot.className = 'status-dot connected';
        text.textContent = 'Connected';
        addLogEntry('INFO', 'WebSocket connected to server');
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            handleWsEvent(data);
        } catch {
            addLogEntry('DEBUG', event.data);
        }
    };

    ws.onclose = () => {
        dot.className = 'status-dot disconnected';
        text.textContent = 'Disconnected';
        addLogEntry('WARN', 'WebSocket disconnected — reconnecting in 3s…');
        // Auto-reconnect.
        setTimeout(connectWebSocket, 3000);
    };

    ws.onerror = () => {
        dot.className = 'status-dot disconnected';
        text.textContent = 'Error';
    };
}

/**
 * Handle a parsed WebSocket event.
 * @param {object} data - Parsed JSON event
 */
function handleWsEvent(data) {
    const type = data.type || 'unknown';

    switch (type) {
        case 'service_started':
        case 'service_stopped':
        case 'service_restarted':
            addLogEntry('INFO', `${data.service}: ${type.replace('service_', '')}`, data.timestamp);
            if (currentPage === 'services') fetchServices();
            if (currentPage === 'overview') fetchStatus();
            break;

        case 'device_connected':
        case 'device_disconnected':
        case 'device_programmed':
            addLogEntry('INFO', `Device ${data.device_serial || data.serial}: ${type.replace('device_', '')}`, data.timestamp);
            if (currentPage === 'devices') fetchDevices();
            if (currentPage === 'overview') fetchStatus();
            break;

        // Resource lifecycle events (Phase 25-Fix-H, F9 + Phase 28D).
        case 'resource_registered':
        case 'resource_acquired':
        case 'resource_released':
        case 'resource_deleted':
        case 'resource_locked':
        case 'resource_unlocked':
        case 'resources_cleared':
            addLogEntry('INFO', `Resource event: ${type}`, data.timestamp);
            // Cross-page refresh: resource changes affect both Resources and Users pages.
            if (currentPage === 'resources') fetchResources();
            if (currentPage === 'users') fetchUsers();
            if (currentPage === 'overview') fetchStatus();
            break;

        // User lifecycle events (Phase 25-Fix-H, F9).
        case 'user_connected':
        case 'user_disconnected':
            addLogEntry('INFO', `User event: ${type} (${data.user_id || ''})`, data.timestamp);
            // Cross-page refresh: user changes affect both Users and Resources pages.
            if (currentPage === 'users') fetchUsers();
            if (currentPage === 'resources') fetchResources();
            if (currentPage === 'overview') fetchStatus();
            break;

        case 'log':
            addLogEntry(data.level || 'INFO', data.message || '', data.timestamp);
            break;

        case 'warning':
            addLogEntry('WARN', data.message || 'Unknown warning');
            break;

        case 'server_shutdown':
            addLogEntry('ERROR', 'Server is shutting down', data.timestamp);
            break;

        // Settings update event (Phase 28B).
        case 'settings_updated':
            addLogEntry('INFO', 'Server settings updated', data.timestamp);
            if (currentPage === 'settings') fetchSettings();
            break;

        default:
            addLogEntry('DEBUG', JSON.stringify(data));
    }
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Set text content of an element by ID.
 * @param {string} id - Element ID
 * @param {*} value - Text value
 */
function setText(id, value) {
    const el = document.getElementById(id);
    if (el) el.textContent = String(value);
}

/**
 * Get an HTML badge for a service status.
 * @param {string} status - Status string
 * @returns {string} HTML badge
 */
function getStatusBadge(status) {
    const lower = status.toLowerCase();
    if (lower.includes('running')) return `<span class="badge badge-running">${status}</span>`;
    if (lower.includes('stopped')) return `<span class="badge badge-stopped">${status}</span>`;
    if (lower.includes('starting')) return `<span class="badge badge-starting">${status}</span>`;
    return `<span class="badge">${status}</span>`;
}

/**
 * Escape HTML entities.
 * @param {string} str - Raw string
 * @returns {string} Escaped string
 */
function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// ============================================================================
// Settings (Phase 28B/28C)
// ============================================================================

/**
 * Fetch current server settings and populate the Settings form.
 *
 * The server description is always returned. The endpoint_enforcement field
 * is only included when `[dashboard] expose_settings = true` — if absent,
 * the enforcement section is hidden from the UI.
 */
async function fetchSettings() {
    try {
        const res = await fetch(`${API_BASE}/settings`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();

        // Description — always available.
        document.getElementById('setting-description').value = data.description || '';

        // Enforcement — only shown when expose_settings is true (field present).
        const enforcementSection = document.getElementById('enforcement-section');
        if (data.endpoint_enforcement !== undefined && data.endpoint_enforcement !== null) {
            document.getElementById('setting-enforcement').value = data.endpoint_enforcement;
            if (enforcementSection) enforcementSection.style.display = '';
        } else {
            if (enforcementSection) enforcementSection.style.display = 'none';
        }
    } catch (err) {
        console.error('Failed to load settings:', err);
    }
}

/**
 * Save settings to the server via PUT.
 *
 * Always sends the description. Only includes endpoint_enforcement if the
 * enforcement section is visible (i.e., expose_settings is true).
 */
async function saveSettings() {
    const description = document.getElementById('setting-description').value;
    const statusPanel = document.getElementById('settings-status-panel');
    const statusMsg = document.getElementById('settings-status-msg');

    const payload = { description };

    // Only include enforcement if the section is visible.
    const enforcementSection = document.getElementById('enforcement-section');
    if (enforcementSection && enforcementSection.style.display !== 'none') {
        payload.endpoint_enforcement = document.getElementById('setting-enforcement').value;
    }

    try {
        const res = await fetch(`${API_BASE}/settings`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        });

        if (!res.ok) {
            const text = await res.text();
            throw new Error(text || `HTTP ${res.status}`);
        }

        statusMsg.textContent = '\u2714 Settings saved successfully and persisted to config file.';
        statusMsg.className = 'settings-status success';
        statusPanel.style.display = '';

        // Auto-hide after 5 seconds.
        setTimeout(() => { statusPanel.style.display = 'none'; }, 5000);
    } catch (err) {
        statusMsg.textContent = `\u2718 Failed to save settings: ${err.message}`;
        statusMsg.className = 'settings-status error';
        statusPanel.style.display = '';
    }
}

// Wire the Save button.
document.getElementById('btn-save-settings')?.addEventListener('click', saveSettings);

/**
 * Check settings visibility for the enforcement section.
 * The Settings nav item is always visible since the description is always
 * accessible. This function is called at init to pre-check whether the
 * enforcement dropdown should be shown.
 */
async function checkSettingsVisibility() {
    // No-op: Settings page is always accessible (description is always available).
    // The enforcement section visibility is handled by fetchSettings().
}

// ============================================================================
// Initialization
// ============================================================================

(function init() {
    // Initial data fetch.
    fetchStatus();

    // Start status polling.
    statusPollTimer = setInterval(fetchStatus, STATUS_POLL_INTERVAL);

    // Connect WebSocket.
    connectWebSocket();

    // Show initial log message.
    addLogEntry('INFO', 'Dashboard initialized');

    // Fetch initial log file size (ADR-022).
    updateLogFileSize();

    // Check if Settings page should be visible (Phase 28B).
    checkSettingsVisibility();

    // Fetch and cache the catalog (Phase 28D).
    api(`${API_BASE}/catalog`).then(data => {
        if (data) catalogCache = data;
    });
})();
