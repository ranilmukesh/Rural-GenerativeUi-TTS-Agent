import { useState, useRef, useEffect } from 'react';
// onBack prop: called when user clicks "← Results"
import { Mic, ArrowRight, ThumbsUp, ThumbsDown, Copy, MoreHorizontal, Sparkles } from 'lucide-react';
import { apiChatStart, apiChatMessage, apiChatTranscribe, apiChatAudio } from '../api.js';
import { formatChatMarkdown, playAudioBase64 } from '../utils.js';
import { Waveform, MiniGame, Message } from './common';

/* ── Waveform bars ─────────────────────────────────────────── */


/* ── Main ChatView ──────────────────────────────────────────── */
export default function ChatView({
  studentData, prediction, explanation, whatif,
  onScoreboardUpdate, onStatusChange, onBack,
  // Lifted state from App — persists across view switches
  messages, setMessages,
  sessionId, setSessionId,
  initialized, setInitialized,
  isTyping, setIsTyping,             // lifted — survives view switches
}) {
  const [input, setInput] = useState('');
  const [recordingMode, setRecordingMode] = useState(null); // 'stt' | 'live' | null
  const msgsRef = useRef(null);
  const mrRef = useRef(null);
  const chunksRef = useRef([]);
  // useRef guards — synchronous, immune to React 18 Strict Mode double-effect timing
  const initDoneRef = useRef(false);   // prevents double apiChatStart (which resets messages)
  const isSendingRef = useRef(false);  // prevents double sendMessage when Enter + button both fire

  // Auto-scroll
  useEffect(() => {
    if (msgsRef.current) msgsRef.current.scrollTop = msgsRef.current.scrollHeight;
  }, [messages, isTyping]);

  // Init chat ONCE per session
  // initDoneRef (not initialized prop) is the primary guard — refs survive the
  // Strict Mode unmount-remount cycle synchronously, so the second effect run
  // always sees initDoneRef.current === true and exits immediately.
  useEffect(() => {
    if (initDoneRef.current || initialized) return;
    initDoneRef.current = true;
    setInitialized(true);
    setIsTyping(true);
    onStatusChange?.('Connecting...');
    apiChatStart({ student_data: studentData, prediction, explanation, whatif: whatif || {} })
      .then(data => {
        setSessionId(data.session_id);
        setMessages([{ type: 'ai', text: data.message }]);
        if (data.audio_base64) playAudioBase64(data.audio_base64);
        if (data.points_update) onScoreboardUpdate?.(data.new_points, data.new_level, data.new_kurals);
        onStatusChange?.('Online');
      })
      .catch(() => {
        setMessages([{ type: 'system', text: '⚠️ Could not connect to Paati AI. Ensure the backend is running.' }]);
        onStatusChange?.('Offline');
      })
      .finally(() => setIsTyping(false));
  }, []); // empty deps — initDoneRef is the reliable guard

  async function sendMessage(text) {
    const msg = (text || input).trim();
    // isSendingRef prevents double-fire: Enter keydown + button click can both
    // fire in the same tick with the same stale `input` value before setInput('')
    // applies, sending the same message twice to the LLM.
    if (!msg || !sessionId || isSendingRef.current) return;
    isSendingRef.current = true;
    setMessages(p => [...p, { type: 'user', text: msg }]);
    setInput('');
    setIsTyping(true);
    onStatusChange?.('Thinking...');
    try {
      const data = await apiChatMessage(sessionId, msg);
      setMessages(p => [...p, { type: 'ai', text: data.response }]);
      if (data.audio_base64) playAudioBase64(data.audio_base64);
      if (data.points_update) onScoreboardUpdate?.(data.new_points, data.new_level, data.new_kurals);
      onStatusChange?.('Online');
    } catch {
      setMessages(p => [...p, { type: 'system', text: '⚠️ Failed to get a response. Please try again.' }]);
      onStatusChange?.('Online');
    } finally {
      setIsTyping(false);
      isSendingRef.current = false;
    }
  }

  async function startRecording(mode) {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mr = new MediaRecorder(stream);
      chunksRef.current = [];
      mr.ondataavailable = e => { if (e.data.size > 0) chunksRef.current.push(e.data); };
      mr.onstop = () => {
        setRecordingMode(null);
        const blob = new Blob(chunksRef.current, { type: 'audio/webm' });
        if (mode === 'stt') processStt(blob);
        else processLive(blob);
      };
      mr.start();
      mrRef.current = mr;
      setRecordingMode(mode);
      onStatusChange?.(mode === 'stt' ? 'Listening (STT)...' : '🎙️ Live Conversation...');
    } catch {
      setMessages(p => [...p, { type: 'system', text: '⚠️ Microphone access denied.' }]);
    }
  }

  function stopRecording() { mrRef.current?.stop(); }
  function toggleRecording(mode) { recordingMode ? stopRecording() : startRecording(mode); }

  async function processStt(blob) {
    onStatusChange?.('Transcribing...');
    try {
      const d = await apiChatTranscribe(blob);
      if (d.transcript?.trim()) setInput(d.transcript);
      onStatusChange?.('Online');
    } catch { onStatusChange?.('Online'); }
  }

  async function processLive(blob) {
    setIsTyping(true);
    onStatusChange?.('Listening & Thinking...');
    try {
      const d = await apiChatAudio(blob, sessionId);
      setMessages(p => [...p,
      { type: 'user', text: `🎤 ${d.transcript}` },
      { type: 'ai', text: d.response },
      ]);
      if (d.audio_base64) playAudioBase64(d.audio_base64);
      if (d.points_update) onScoreboardUpdate?.(d.new_points, d.new_level, d.new_kurals);
      onStatusChange?.('Online');
    } catch {
      setMessages(p => [...p, { type: 'system', text: '⚠️ Failed to send live message.' }]);
      onStatusChange?.('Online');
    } finally { setIsTyping(false); }
  }

  function handleGameComplete(gd) {
    const msg = `I completed the '${gd.title}' mini-game! I finished all ${gd.steps.length} steps and earned the ${gd.final_reward_badge} badge. Please analyze my performance!`;
    sendMessage(msg);
  }

  const hasMessages = messages.length > 0;

  return (
    <main style={{
      flex: 1, display: 'flex', flexDirection: 'column', height: '100%',
      overflow: 'hidden', background: '#f8f9fc', position: 'relative',
    }}>
      {/* ── Header ── */}
      <header style={{ padding: '32px 40px 16px', display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between' }}>
        <div>
          <h1 style={{ fontFamily: 'Space Grotesk, sans-serif', fontSize: '1.8rem', fontWeight: 800, color: '#1e293b', display: 'flex', alignItems: 'center', gap: 10 }}>
            Hi, Student <span className="animate-wave" style={{ fontSize: '1.5rem', display: 'inline-block' }}>👋</span>
          </h1>
          <p style={{ color: '#64748b', marginTop: 6, fontSize: '1rem' }}>How may I help you today?</p>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          {onBack && (
            <button onClick={onBack} style={{
              display: 'flex', alignItems: 'center', gap: 6, padding: '7px 14px',
              background: 'white', border: '1px solid #e2e8f0', borderRadius: 99,
              fontSize: 13, fontWeight: 600, color: '#64748b', cursor: 'pointer',
            }}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
              Results
            </button>
          )}
          <button style={{ width: 32, height: 32, borderRadius: '50%', border: '1px solid #e2e8f0', background: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'pointer', color: '#94a3b8' }}>
            <MoreHorizontal size={16} />
          </button>
        </div>
      </header>

      {/* ── Scrollable message area ── */}
      <div ref={msgsRef} style={{ flex: 1, overflowY: 'auto', padding: '0 40px 120px', display: 'flex', flexDirection: 'column', gap: 16 }} className="scrollbar-hide">

        {/* Orb greeting when empty */}
        {!hasMessages && !isTyping && (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', paddingTop: 20, paddingBottom: 24 }}>
            <div style={{ position: 'relative', marginBottom: 24 }}>
              <div style={{
                width: 80, height: 80, borderRadius: '50%',
                background: 'linear-gradient(135deg, #ffb4c2, #f84d85, #5b4eff)',
                boxShadow: '0 15px 40px -5px rgba(248,77,133,0.5)',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 40, position: 'relative', overflow: 'hidden',
              }}>
                <div style={{ position: 'absolute', top: 4, left: 8, width: 52, height: 28, background: 'rgba(255,255,255,0.28)', borderRadius: '50%', filter: 'blur(2px)', transform: 'rotate(-12deg)' }} />
                👵
              </div>
              {/* Speech bubble */}
              <div style={{
                position: 'absolute', top: -8, right: -160,
                background: 'white', border: '1px solid #f1f5f9',
                boxShadow: '0 4px 12px rgba(0,0,0,0.06)', borderRadius: '16px 16px 16px 4px',
                padding: '8px 16px', whiteSpace: 'nowrap',
              }}>
                <span style={{ fontSize: 12, fontWeight: 600, color: '#475569' }}>Let's solve this together! 🎯</span>
              </div>
            </div>
          </div>
        )}

        {/* Typing indicator (initial load) */}
        {isTyping && messages.length === 0 && (
          <div style={{ maxWidth: 580 }}>
            <div style={{ background: 'white', border: '1px solid #f1f5f9', padding: '12px 16px', borderRadius: '18px 18px 18px 4px', boxShadow: '0 2px 8px rgba(0,0,0,0.04)', display: 'flex', gap: 6, alignItems: 'center' }}>
              {[0, 150, 300].map(d => (
                <div key={d} style={{ width: 8, height: 8, background: '#6366f1', borderRadius: '50%', animation: `bounce 1.2s ease-in-out ${d}ms infinite` }} />
              ))}
            </div>
          </div>
        )}

        {/* Messages */}
        {messages.map((m, i) => (
          <Message key={i} msg={m} onGameComplete={handleGameComplete} />
        ))}

        {/* Typing indicator (mid-conversation) */}
        {isTyping && messages.length > 0 && (
          <div style={{ maxWidth: 580 }}>
            <div style={{ background: 'white', border: '1px solid #f1f5f9', padding: '12px 16px', borderRadius: '18px 18px 18px 4px', boxShadow: '0 2px 8px rgba(0,0,0,0.04)', display: 'flex', gap: 6, alignItems: 'center' }}>
              {[0, 150, 300].map(d => (
                <div key={d} style={{ width: 8, height: 8, background: '#6366f1', borderRadius: '50%', animation: `bounce 1.2s ease-in-out ${d}ms infinite` }} />
              ))}
            </div>
          </div>
        )}
      </div>

      {/* ── Fixed bottom input bar ── */}
      <div style={{
        position: 'absolute', bottom: 0, left: 0, right: 0,
        background: 'linear-gradient(to top, #f8f9fc 70%, transparent)',
        padding: '20px 40px 24px',
        display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 12,
      }}>
        <div style={{
          width: '100%', maxWidth: 680,
          display: 'flex', alignItems: 'center',
          background: 'white', border: '1px solid rgba(226,232,240,0.8)',
          borderRadius: 20, padding: 10,
          boxShadow: '0 8px 30px rgba(0,0,0,0.06)',
        }}>
          {/* Mic (STT) */}
          <button
            onClick={() => toggleRecording('stt')}
            title="Voice to text"
            style={{
              width: 44, height: 44, borderRadius: 14, border: 'none', cursor: 'pointer',
              background: recordingMode === 'stt' ? '#ef4444' : '#6366f1',
              color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0, boxShadow: '0 2px 8px rgba(99,102,241,0.3)',
              animation: recordingMode === 'stt' ? 'pulse-anim 0.8s ease-in-out infinite' : 'none',
            }}>
            <Mic size={20} />
          </button>

          {/* Input + waveform overlay */}
          <div style={{ flex: 1, position: 'relative', height: 44, margin: '0 10px' }}>
            <input
              type="text"
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); } }}
              placeholder={recordingMode ? 'Listening… click mic to stop' : 'Tap mic and start talking...'}
              disabled={recordingMode !== null}
              style={{
                position: 'absolute', inset: 0, width: '100%', height: '100%',
                background: 'transparent', border: 'none', outline: 'none',
                fontFamily: 'Inter, sans-serif', fontSize: 14, color: '#334155',
                paddingLeft: 4,
              }}
            />
            {/* Waveform overlay when recording */}
            {recordingMode && (
              <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', pointerEvents: 'none', opacity: 0.5 }}>
                <Waveform bars={40} active={true} />
              </div>
            )}
          </div>

          {/* Live voice button */}
          <button
            onClick={() => toggleRecording('live')}
            title="Live voice conversation"
            style={{
              width: 44, height: 44, borderRadius: 14, border: 'none', cursor: 'pointer',
              background: recordingMode === 'live' ? '#8b5cf6' : 'rgba(99,102,241,0.08)',
              color: recordingMode === 'live' ? 'white' : '#6366f1',
              display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0,
              animation: recordingMode === 'live' ? 'pulse-anim 0.8s ease-in-out infinite' : 'none',
            }}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="18" height="18">
              <path d="M2 10v3" /><path d="M6 6v11" /><path d="M10 3v18" /><path d="M14 8v7" /><path d="M18 5v13" /><path d="M22 10v3" />
            </svg>
          </button>

          {/* Send */}
          <button
            onClick={() => sendMessage()}
            disabled={!input.trim() || !sessionId || isTyping}
            style={{
              width: 44, height: 44, borderRadius: 14, border: 'none',
              cursor: input.trim() && sessionId && !isTyping ? 'pointer' : 'default',
              background: input.trim() && sessionId && !isTyping ? '#6366f1' : 'rgba(226,232,240,0.6)',
              color: input.trim() && sessionId && !isTyping ? 'white' : '#94a3b8',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0, marginLeft: 8,
              boxShadow: input.trim() && sessionId && !isTyping ? '0 2px 8px rgba(99,102,241,0.3)' : 'none',
              transition: 'all 0.2s',
            }}>
            <ArrowRight size={20} />
          </button>
        </div>

        <p style={{ fontSize: 11, color: '#94a3b8', fontWeight: 500, letterSpacing: '0.04em' }}>
          Smart Assistant can make mistakes. Please verify important information.
        </p>
      </div>
    </main>
  );
}
