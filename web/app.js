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
 * @param {string} page - One of: overview, devices, resources, users, services, logs
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
        case 'services': fetchServices(); break;
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
    setText('detail-fp-version', data.frontpanel_version);
    setText('detail-platform', data.platform);
    setText('detail-features', (data.compiled_features || []).join(', ') || 'none');

    // Fetch config for management port.
    const cfg = await api(`${API_BASE}/config`);
    if (cfg) {
        setText('detail-mgmt-port', cfg.management_port);
    }
}

// ============================================================================
// Devices Page
// ============================================================================

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

        tbody.innerHTML = devices.map(d => `
            <tr>
                <td><code>${escapeHtml(d.serial)}</code></td>
                <td>${escapeHtml(d.product_name)}</td>
                <td>${escapeHtml(d.interface)}</td>
                <td>${escapeHtml(d.usb_speed)}</td>
                <td><span class="badge badge-${d.state.startsWith('Programmed') ? 'success' : 'info'}">${escapeHtml(d.state)}</span></td>
            </tr>`).join('');
    }
}

document.getElementById('btn-refresh-devices')?.addEventListener('click', fetchDevices);

// ============================================================================
// Resources Page
// ============================================================================

async function fetchResources() {
    const data = await api(`${API_BASE}/status`);
    if (!data) return;

    const table = document.getElementById('resources-table');
    const empty = document.getElementById('resources-empty');

    if (data.active_resources === 0) {
        table.style.display = 'none';
        empty.style.display = 'block';
    } else {
        table.style.display = 'table';
        empty.style.display = 'none';
    }
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
// Log Viewer
// ============================================================================

const logContainer = document.getElementById('log-container');
const autoScrollCheckbox = document.getElementById('log-auto-scroll');

document.getElementById('btn-clear-logs')?.addEventListener('click', () => {
    logContainer.innerHTML = '';
});

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
            addLogEntry('INFO', `Device ${data.serial}: ${type.replace('device_', '')}`, data.timestamp);
            if (currentPage === 'devices') fetchDevices();
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
})();
