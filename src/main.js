const { invoke } = window.__TAURI__.core;

let timelineEl;
let statusEl;
let statusTextEl;
let mqttStatusEl;
let timelineView;
let settingsView;
let pollInterval;

// Format timestamp to HH:MM:SS
function formatTime(timestamp) {
  const date = new Date(timestamp);
  return date.toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  });
}

// Render a single voice entry
function renderEntry(entry) {
  return `
    <div class="voice-entry ${entry.status}">
      <div class="time">${formatTime(entry.timestamp)}</div>
      <div class="content">
        <div class="text">${escapeHtml(entry.text)}</div>
        <div class="meta">
          ${entry.agent ? `<span class="agent">${escapeHtml(entry.agent)}</span>` : ''}
          <span class="voice-name">${escapeHtml(entry.voice)}</span>
        </div>
      </div>
    </div>
  `;
}

// Escape HTML to prevent XSS, convert \n to <br>
function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  // Convert literal \n to <br> for display
  return div.innerHTML.replace(/\\n/g, '<br>').replace(/\n/g, '<br>');
}

// Update the timeline UI
async function updateTimeline() {
  try {
    const timeline = await invoke('get_timeline');
    const status = await invoke('get_status');

    // Update MQTT status indicator
    const mqttStatus = status.mqtt_status || 'disconnected';
    mqttStatusEl.className = `mqtt-status ${mqttStatus}`;
    mqttStatusEl.title = `MQTT: ${mqttStatus}`;

    // Show label for all states
    const mqttLabel = document.getElementById('mqtt-label');
    if (mqttStatus === 'connected') {
      mqttLabel.textContent = 'connected';
    } else if (mqttStatus === 'connecting') {
      mqttLabel.textContent = 'connecting...';
    } else {
      mqttLabel.textContent = 'offline';
    }

    // Update status indicator
    if (status.is_speaking) {
      statusEl.className = 'status speaking';
      statusTextEl.textContent = 'Speaking...';
    } else if (status.queued > 0) {
      statusEl.className = 'status queued';
      statusTextEl.textContent = `${status.queued} queued`;
    } else {
      statusEl.className = 'status';
      statusTextEl.textContent = 'Idle';
    }

    // Update timeline
    if (timeline.length === 0) {
      timelineEl.innerHTML = '<div class="empty-state">No voice messages yet</div>';
    } else {
      // Show newest first
      const reversed = [...timeline].reverse();
      timelineEl.innerHTML = reversed.map(renderEntry).join('');
    }
  } catch (err) {
    console.error('Failed to update timeline:', err);
  }
}

// Test voice using Tauri command
async function testVoice() {
  try {
    await invoke('test_voice');
    // Immediately refresh
    setTimeout(updateTimeline, 100);
  } catch (err) {
    console.error('Failed to test voice:', err);
  }
}

// Clear done entries
async function clearDone() {
  try {
    await invoke('clear_timeline');
    updateTimeline();
  } catch (err) {
    console.error('Failed to clear timeline:', err);
  }
}

// Show settings view
async function showSettings() {
  // Stop polling when in settings
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }

  // Load current config
  try {
    const config = await invoke('get_mqtt_config');
    document.getElementById('broker').value = config.broker;
    document.getElementById('port').value = config.port;
    document.getElementById('topic-speak').value = config.topic_speak;
    document.getElementById('topic-status').value = config.topic_status;
    document.getElementById('username').value = config.username || '';
    document.getElementById('password').value = config.password || '';
  } catch (err) {
    console.error('Failed to load config:', err);
  }

  // Hide message
  const msgEl = document.getElementById('settings-message');
  msgEl.classList.add('hidden');

  // Toggle views
  timelineView.classList.add('hidden');
  settingsView.classList.remove('hidden');
}

// Show timeline view
function showTimeline() {
  timelineView.classList.remove('hidden');
  settingsView.classList.add('hidden');

  // Resume polling and immediately update
  updateTimeline();
  if (!pollInterval) {
    pollInterval = setInterval(updateTimeline, 500);
  }
}

// Save settings
async function saveSettings() {
  const broker = document.getElementById('broker').value.trim();
  const port = parseInt(document.getElementById('port').value, 10);
  const topicSpeak = document.getElementById('topic-speak').value.trim();
  const topicStatus = document.getElementById('topic-status').value.trim();
  const username = document.getElementById('username').value.trim();
  const password = document.getElementById('password').value;

  // Validation
  if (!broker) {
    showMessage('Broker is required', 'error');
    return;
  }
  if (isNaN(port) || port < 1 || port > 65535) {
    showMessage('Port must be 1-65535', 'error');
    return;
  }
  if (!topicSpeak) {
    showMessage('Speak topic is required', 'error');
    return;
  }

  try {
    await invoke('save_mqtt_config', {
      config: {
        broker,
        port,
        topic_speak: topicSpeak,
        topic_status: topicStatus || 'voice/status',
        username: username || null,
        password: password || null
      }
    });
    // Go back to main view after successful save
    showTimeline();
  } catch (err) {
    showMessage('Failed to save: ' + err, 'error');
  }
}

// Show message in settings
function showMessage(text, type) {
  const msgEl = document.getElementById('settings-message');
  msgEl.textContent = text;
  msgEl.className = 'settings-message ' + type;
  msgEl.classList.remove('hidden');
}

window.addEventListener("DOMContentLoaded", () => {
  timelineEl = document.getElementById('timeline');
  statusEl = document.getElementById('status');
  statusTextEl = document.getElementById('status-text');
  mqttStatusEl = document.getElementById('mqtt-status');
  timelineView = document.getElementById('timeline-view');
  settingsView = document.getElementById('settings-view');

  // Initial load
  updateTimeline();

  // Poll for updates every 500ms
  pollInterval = setInterval(updateTimeline, 500);

  // Button handlers - Timeline
  document.getElementById('test-btn').addEventListener('click', testVoice);
  document.getElementById('clear-btn').addEventListener('click', clearDone);
  document.getElementById('settings-btn').addEventListener('click', showSettings);

  // Button handlers - Settings
  document.getElementById('cancel-btn').addEventListener('click', showTimeline);
  document.getElementById('save-btn').addEventListener('click', saveSettings);
});
