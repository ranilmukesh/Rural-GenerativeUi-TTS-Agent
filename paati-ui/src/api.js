/**
 * Paati-Kural API Service
 * Mirrors all calls from the original script.js
 */

export const API_BASE_URL =
  window.location.hostname === '127.0.0.1' || window.location.hostname === 'localhost'
    ? window.location.protocol + '//' + window.location.hostname + ':8000'
    : window.location.origin;

export async function apiHealth() {
  const r = await fetch(`${API_BASE_URL}/health`);
  return r.json();
}

export async function apiOptions() {
  const r = await fetch(`${API_BASE_URL}/options`);
  return r.json();
}

export async function apiPredict(data) {
  const r = await fetch(`${API_BASE_URL}/predict`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiExplain(data) {
  const r = await fetch(`${API_BASE_URL}/explain`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiWhatIf(data) {
  const r = await fetch(`${API_BASE_URL}/whatif`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiUploadResume(file) {
  const fd = new FormData();
  fd.append('file', file);
  const r = await fetch(`${API_BASE_URL}/upload/resume`, { method: 'POST', body: fd });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiChatStart(payload) {
  const r = await fetch(`${API_BASE_URL}/chat/start`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiChatMessage(sessionId, message) {
  const r = await fetch(`${API_BASE_URL}/chat/message`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, message }),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiChatTranscribe(audioBlob) {
  const fd = new FormData();
  fd.append('audio_file', audioBlob, 'stt_note.webm');
  const r = await fetch(`${API_BASE_URL}/chat/transcribe`, { method: 'POST', body: fd });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function apiChatAudio(audioBlob, sessionId) {
  const fd = new FormData();
  fd.append('audio_file', audioBlob, 'live_voice.webm');
  fd.append('session_id', sessionId);
  const r = await fetch(`${API_BASE_URL}/chat/audio`, { method: 'POST', body: fd });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}
