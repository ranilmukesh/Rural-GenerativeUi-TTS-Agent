import { useState, useRef, useEffect } from 'react';
// onBack prop: called when user clicks "← Results"
import { Mic, ArrowRight, ThumbsUp, ThumbsDown, Copy, MoreHorizontal, Sparkles } from 'lucide-react';
import { apiChatStart, apiChatMessage, apiChatTranscribe, apiChatAudio } from '../api.js';
import { formatChatMarkdown, playAudioBase64 } from '../utils.js';

/* ── Waveform bars ─────────────────────────────────────────── */
function Waveform({ bars = 32, active = false }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 2, height: '100%', width: '100%' }}>
      {Array.from({ length: bars }).map((_, i) => {
        const h = active
          ? Math.random() * 80 + 20
          : 20 + Math.sin(i * 0.8) * 18 + Math.abs(Math.sin(i * 0.4)) * 38;
        return (
          <div key={i} style={{
            flex: 1, height: `${h}%`, borderRadius: 2,
            background: 'rgba(148,163,184,0.5)',
            transition: active ? 'height 0.12s ease' : 'none',
          }} />
        );
      })}
    </div>
  );
}

/* ── Mini-game renderer ─────────────────────────────────────── */
function MiniGame({ gameData, onComplete }) {
  const [step, setStep] = useState(0);
  const [seqOrder, setSeqOrder] = useState([]);

  if (step >= gameData.steps.length) {
    return (
      <div style={{ background: 'linear-gradient(135deg,#f8f9ff,#f1f4ff)', border: '2px solid #e5e9ff', borderRadius: 16, padding: 20 }}>
        <h4 style={{ color: '#22c55e', marginBottom: 8 }}>🏆 Game Complete!</h4>
        <p style={{ marginBottom: 14, fontSize: 14 }}>You earned the <strong>{gameData.final_reward_badge}</strong> badge!</p>
        <button onClick={() => onComplete(gameData)} style={{
          width: '100%', padding: '10px 0', background: '#5b4eff', color: 'white',
          border: 'none', borderRadius: 10, fontWeight: 700, cursor: 'pointer', fontSize: 14,
        }}>Claim Reward & Continue Chat</button>
      </div>
    );
  }

  const s = gameData.steps[step];

  return (
    <div style={{ background: 'white', border: '1px solid #e8eaff', borderRadius: 20, padding: 20, boxShadow: '0 8px 30px rgba(0,0,0,0.04)' }}>
      {/* Badge */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
        <span style={{ fontSize: 11, fontWeight: 700, color: '#6366f1', textTransform: 'uppercase', letterSpacing: '0.08em' }}>
          {gameData.title}
        </span>
        <span style={{ fontSize: 11, background: '#ede9fe', color: '#6366f1', padding: '2px 8px', borderRadius: 99, fontWeight: 600 }}>
          ✨ Generative Game
        </span>
      </div>

      <div style={{ fontSize: '2.5rem', textAlign: 'center', margin: '12px 0' }}>{s.visual}</div>
      <p style={{ fontStyle: 'italic', fontSize: 13, color: '#475569', marginBottom: 14, lineHeight: 1.5 }}>👵 "{s.paati_says}"</p>
      {s.question && <p style={{ fontWeight: 600, fontSize: 14, marginBottom: 14 }}>{s.question}</p>}

      {/* Sequence */}
      {s.step_type === 'sequence' && s.sequence_items && (
        <>
          <p style={{ fontSize: 12, color: '#94a3b8', marginBottom: 8 }}>Click in correct order:</p>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, padding: 12, background: '#f8f9fc', borderRadius: 12, border: '2px solid transparent' }}>
            {s.sequence_items.map((item, i) => (
              <div key={i} onClick={() => {
                if (seqOrder.includes(item)) return;
                const next = [...seqOrder, item];
                setSeqOrder(next);
                if (next.length === s.sequence_items.length) {
                  const ok = next.every((v, j) => v === s.correct_sequence[j]);
                  setTimeout(() => { if (ok) setStep(c => c + 1); setSeqOrder([]); }, 800);
                }
              }} style={{
                padding: '6px 14px', borderRadius: 8, cursor: 'pointer', fontSize: 13, fontWeight: 500,
                background: seqOrder.includes(item) ? '#5b4eff' : 'white',
                color: seqOrder.includes(item) ? 'white' : '#1e293b',
                border: '1px solid #e2e8f0', transition: '0.2s', userSelect: 'none',
              }}>{item}</div>
            ))}
          </div>
        </>
      )}

      {/* Options Quiz */}
      {s.options?.length > 0 && s.step_type !== 'sequence' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {s.options.map((opt, i) => (
            <button key={i} onClick={() => {
              if (opt === s.correct_answer) setTimeout(() => setStep(c => c + 1), 800);
            }} style={{
              padding: '10px 14px', background: '#f8f9fc', border: '1px solid #e2e8f0',
              borderRadius: 10, cursor: 'pointer', textAlign: 'left', fontSize: 13, fontWeight: 500,
              transition: '0.2s', color: '#1e293b',
            }}>{opt}</button>
          ))}
        </div>
      )}

      {/* Dialogue */}
      {s.step_type === 'dialogue' && !s.options?.length && !s.sequence_items && (
        <button onClick={() => setStep(c => c + 1)} style={{
          width: '100%', padding: 10, background: '#5b4eff', color: 'white',
          border: 'none', borderRadius: 10, cursor: 'pointer', fontWeight: 600, fontSize: 14,
        }}>Next →</button>
      )}
    </div>
  );
}

/* ── Single message bubble ──────────────────────────────────── */
function Message({ msg, onGameComplete }) {
  // Try parse mini-game
  if (msg.type === 'ai' && msg.text?.includes('"step_type"') && msg.text?.includes('"steps"')) {
    try {
      const match = msg.text.match(/```json\s*([\s\S]*?)\s*```/) ||
        msg.text.match(/```\s*([\s\S]*?)\s*```/) ||
        msg.text.match(/(\{[\s\S]*"steps"[\s\S]*\})/);
      if (match) {
        const gd = JSON.parse(match[1]);
        if (gd.steps && gd.title) {
          const textBefore = msg.text.split(/```json|\{/)[0].trim();
          return (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10, alignItems: 'flex-start', maxWidth: 560 }}>
              {textBefore && (
                <div style={{ background: 'white', border: '1px solid #f1f5f9', borderRadius: '18px 18px 18px 4px', padding: '12px 16px', boxShadow: '0 2px 8px rgba(0,0,0,0.04)', fontSize: 14, color: '#334155' }}
                  dangerouslySetInnerHTML={{ __html: formatChatMarkdown(textBefore) }} />
              )}
              <div style={{ width: '100%' }}>
                <MiniGame gameData={gd} onComplete={onGameComplete} />
              </div>
            </div>
          );
        }
      }
    } catch { }
  }

  if (msg.type === 'user') {
    return (
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <div style={{
          background: 'linear-gradient(135deg,#6366f1,#5b4eff)', color: 'white',
          padding: '10px 16px', borderRadius: '18px 18px 4px 18px',
          maxWidth: '75%', fontSize: 14, lineHeight: 1.6,
          boxShadow: '0 4px 12px rgba(99,102,241,0.25)',
        }} dangerouslySetInnerHTML={{ __html: formatChatMarkdown(msg.text) }} />
      </div>
    );
  }

  if (msg.type === 'system') {
    return (
      <div style={{ display: 'flex', justifyContent: 'center' }}>
        <div style={{ background: 'rgba(239,68,68,0.08)', color: '#ef4444', padding: '8px 16px', borderRadius: 10, fontSize: 12 }}
          dangerouslySetInnerHTML={{ __html: formatChatMarkdown(msg.text) }} />
      </div>
    );
  }

  // AI message
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-start', maxWidth: 580 }}>
      <div style={{
        background: 'white', border: '1px solid #f1f5f9',
        padding: '12px 16px', borderRadius: '18px 18px 18px 4px',
        fontSize: 14, color: '#334155', lineHeight: 1.65,
        boxShadow: '0 2px 12px rgba(0,0,0,0.04)',
      }} dangerouslySetInnerHTML={{ __html: formatChatMarkdown(msg.text) }} />
      {/* Feedback row */}
      <div style={{ display: 'flex', gap: 4, paddingLeft: 4 }}>
        {[ThumbsUp, ThumbsDown, Copy].map((Icon, i) => (
          <button key={i} style={{ padding: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: '#94a3b8', borderRadius: 6, display: 'flex', alignItems: 'center' }}>
            <Icon size={14} />
          </button>
        ))}
      </div>
    </div>
  );
}

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

  // Auto-scroll
  useEffect(() => {
    if (msgsRef.current) msgsRef.current.scrollTop = msgsRef.current.scrollHeight;
  }, [messages, isTyping]);

  // Init chat ONCE per session — guard with lifted `initialized` flag
  useEffect(() => {
    if (initialized) return;          // already done — don't re-fire
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
  }, []); // empty deps — runs only on first mount; guard handles re-mounts

  async function sendMessage(text) {
    const msg = (text || input).trim();
    if (!msg || !sessionId) return;
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
    } finally { setIsTyping(false); }
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
            disabled={!input.trim() || !sessionId}
            style={{
              width: 44, height: 44, borderRadius: 14, border: 'none', cursor: input.trim() && sessionId ? 'pointer' : 'default',
              background: input.trim() && sessionId ? '#6366f1' : 'rgba(226,232,240,0.6)',
              color: input.trim() && sessionId ? 'white' : '#94a3b8',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0, marginLeft: 8,
              boxShadow: input.trim() && sessionId ? '0 2px 8px rgba(99,102,241,0.3)' : 'none',
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
