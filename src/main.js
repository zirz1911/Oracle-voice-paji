const { invoke } = window.__TAURI__.core;

let timelineEl;
let statusEl;
let statusTextEl;

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

// Escape HTML to prevent XSS
function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// Update the timeline UI
async function updateTimeline() {
  try {
    const timeline = await invoke('get_timeline');
    const status = await invoke('get_status');

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

window.addEventListener("DOMContentLoaded", () => {
  timelineEl = document.getElementById('timeline');
  statusEl = document.getElementById('status');
  statusTextEl = document.getElementById('status-text');

  // Initial load
  updateTimeline();

  // Poll for updates every 500ms
  setInterval(updateTimeline, 500);

  // Button handlers
  document.getElementById('test-btn').addEventListener('click', testVoice);
  document.getElementById('clear-btn').addEventListener('click', clearDone);
});
