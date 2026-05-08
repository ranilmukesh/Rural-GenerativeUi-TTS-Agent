import { useState, useRef, useEffect } from 'react';
import { apiChatStart, apiChatMessage, apiChatTranscribe, apiChatAudio } from '../api.js';
import { formatChatMarkdown, playAudioBase64 } from '../utils.js';

function MiniGame({ gameData, onComplete }) {
  const [currentStep, setCurrentStep] = useState(0);
  const [selectedOrder, setSelectedOrder] = useState([]);

  if (currentStep >= gameData.steps.length) {
    return (
      <div className="mini-game-container">
        <h4 style={{ margin: '0 0 10px', color: 'var(--accent-teal)' }}>🏆 Game Complete!</h4>
        <p style={{ marginBottom: 15 }}>You earned the <strong>{gameData.final_reward_badge}</strong> badge!</p>
        <button className="submit-btn" style={{ padding: '10px', fontSize: 14 }} onClick={() => onComplete(gameData)}>
          Claim Reward & Send
        </button>
      </div>
    );
  }

  const step = gameData.steps[currentStep];

  function handleOption(val) {
    if (val === step.correct_answer) {
      setTimeout(() => setCurrentStep(c => c + 1), 1000);
    }
  }

  function handleSeqClick(item) {
    if (selectedOrder.includes(item)) return;
    const newOrder = [...selectedOrder, item];
    setSelectedOrder(newOrder);
    if (newOrder.length === step.sequence_items.length) {
      const correct = newOrder.every((v, i) => v === step.correct_sequence[i]);
      if (correct) setTimeout(() => { setCurrentStep(c => c + 1); setSelectedOrder([]); }, 800);
      else setTimeout(() => setSelectedOrder([]), 800);
    }
  }

  return (
    <div className="mini-game-container">
      <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--accent-coral)', textTransform: 'uppercase', marginBottom: 8 }}>
        {gameData.title} — Level: {gameData.level}
      </div>
      <div style={{ fontSize: '2.5rem', textAlign: 'center', margin: '12px 0' }}>{step.visual}</div>
      <p style={{ fontStyle: 'italic', marginBottom: 12, lineHeight: 1.5 }}>👵 "{step.paati_says}"</p>
      {step.question && <p style={{ fontWeight: 600, marginBottom: 12 }}>{step.question}</p>}

      {step.step_type === 'sequence' && step.sequence_items && (
        <>
          <p style={{ fontSize: 13, color: 'var(--gray-500)', marginBottom: 8 }}>Click in correct order:</p>
          <div className="sequence-container">
            {step.sequence_items.map((item, i) => (
              <div key={i} className={`sequence-item ${selectedOrder.includes(item) ? 'selected' : ''}`}
                onClick={() => handleSeqClick(item)}>{item}</div>
            ))}
          </div>
        </>
      )}

      {step.step_type === 'code_debug' && step.code_snippet && (
        <>
          <p style={{ fontSize: 13, color: 'var(--gray-500)', marginBottom: 8 }}>Find the bug! Enter the line number:</p>
          <pre style={{ background: '#1e1e1e', color: '#d4d4d4', padding: 10, borderRadius: 6, fontSize: 12, overflowX: 'auto', marginBottom: 10 }}>
            <code>{step.code_snippet}</code>
          </pre>
          <div style={{ display: 'flex', gap: 8 }}>
            <input type="number" id="debug-line" placeholder="Line #" style={{ flex: 1, padding: 8, borderRadius: 4, border: '1px solid #ccc', background: 'var(--cream)', color: 'black' }} />
            <button style={{ padding: '8px 16px', background: 'var(--accent-teal)', color: 'white', border: 'none', borderRadius: 4, cursor: 'pointer' }}
              onClick={() => {
                const val = parseInt(document.getElementById('debug-line').value);
                if (val === step.bug_line) setTimeout(() => setCurrentStep(c => c + 1), 800);
              }}>Fix Bug</button>
          </div>
        </>
      )}

      {step.options?.length > 0 && step.step_type !== 'sequence' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {step.options.map((opt, i) => (
            <button key={i} onClick={() => handleOption(opt)} style={{
              padding: 10, background: 'var(--cream)', border: '1px solid var(--gray-300)',
              borderRadius: 8, cursor: 'pointer', textAlign: 'left', transition: '0.2s', fontSize: 14,
            }}>{opt}</button>
          ))}
        </div>
      )}

      {step.step_type === 'dialogue' && !step.options?.length && (
        <button className="submit-btn" style={{ padding: 10, fontSize: 14, marginTop: 8 }} onClick={() => setCurrentStep(c => c + 1)}>Next</button>
      )}
    </div>
  );
}

function ChatMessage({ msg, onGameComplete }) {
  // Try to parse mini-game JSON
  if (msg.type === 'ai' && msg.text.includes('"step_type"') && msg.text.includes('"steps"')) {
    try {
      const match = msg.text.match(/```json\s*([\s\S]*?)\s*```/) || msg.text.match(/```\s*([\s\S]*?)\s*```/) || msg.text.match(/(\{[\s\S]*"steps"[\s\S]*\})/);
      if (match) {
        const gameData = JSON.parse(match[1]);
        if (gameData.steps && gameData.title) {
          const textBefore = msg.text.split(/```json|\{/)[0].trim();
          return (
            <div className={`chat-msg chat-msg-${msg.type}`}>
              {textBefore && (
                <div className="chat-bubble" dangerouslySetInnerHTML={{ __html: formatChatMarkdown(textBefore) }} />
              )}
              <MiniGame gameData={gameData} onComplete={onGameComplete} />
            </div>
          );
        }
      }
    } catch { }
  }

  return (
    <div className={`chat-msg chat-msg-${msg.type}`}>
      <div className="chat-bubble" dangerouslySetInnerHTML={{ __html: formatChatMarkdown(msg.text) }} />
    </div>
  );
}

export default function ChatPanel({
  visible, studentData, prediction, explanation, whatif,
  onScoreboardUpdate, onStatusChange,
}) {
  const [messages, setMessages] = useState([]);
  const [inputText, setInputText] = useState('');
  const [sessionId, setSessionId] = useState(null);
  const [initialized, setInitialized] = useState(false);
  const [isTyping, setIsTyping] = useState(false);
  const [isOpen, setIsOpen] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [recordingMode, setRecordingMode] = useState(null);
  const messagesRef = useRef(null);
  const mediaRecorderRef = useRef(null);
  const audioChunksRef = useRef([]);

  useEffect(() => {
    if (messagesRef.current) {
      messagesRef.current.scrollTop = messagesRef.current.scrollHeight;
    }
  }, [messages, isTyping]);

  async function initChat() {
    setInitialized(true);
    setIsTyping(true);
    onStatusChange?.('Connecting...');
    try {
      const data = await apiChatStart({ student_data: studentData, prediction, explanation, whatif: whatif || {} });
      setSessionId(data.session_id);
      setMessages([{ type: 'ai', text: data.message }]);
      if (data.audio_base64) playAudioBase64(data.audio_base64);
      if (data.points_update) onScoreboardUpdate?.(data.new_points, data.new_level, data.new_kurals);
      onStatusChange?.('Online');
    } catch (err) {
      setMessages([{ type: 'system', text: '⚠️ Could not connect to Paati AI.' }]);
      onStatusChange?.('Offline');
    } finally {
      setIsTyping(false);
    }
  }

  function toggleOpen() {
    const opening = !isOpen;
    setIsOpen(opening);
    if (opening && !initialized) initChat();
  }

  async function sendMessage() {
    const msg = inputText.trim();
    if (!msg || !sessionId) return;
    setMessages(prev => [...prev, { type: 'user', text: msg }]);
    setInputText('');
    setIsTyping(true);
    onStatusChange?.('Thinking...');
    try {
      const data = await apiChatMessage(sessionId, msg);
      setMessages(prev => [...prev, { type: 'ai', text: data.response }]);
      if (data.audio_base64) playAudioBase64(data.audio_base64);
      if (data.points_update) onScoreboardUpdate?.(data.new_points, data.new_level, data.new_kurals);
      onStatusChange?.('Online');
    } catch {
      setMessages(prev => [...prev, { type: 'system', text: '⚠️ Failed to get a response.' }]);
      onStatusChange?.('Online');
    } finally {
      setIsTyping(false);
    }
  }

  async function startRecording(mode) {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mr = new MediaRecorder(stream);
      audioChunksRef.current = [];
      mr.ondataavailable = e => { if (e.data.size > 0) audioChunksRef.current.push(e.data); };
      mr.onstop = () => {
        if (mode === 'stt') processStt();
        else processLive();
        setRecordingMode(null);
      };
      mr.start();
      mediaRecorderRef.current = mr;
      setRecordingMode(mode);
      onStatusChange?.(mode === 'stt' ? 'Listening (STT)...' : '🎙️ Live Conversation...');
    } catch {
      setMessages(prev => [...prev, { type: 'system', text: '⚠️ Microphone access denied.' }]);
    }
  }

  function stopRecording() {
    mediaRecorderRef.current?.stop();
  }

  function toggleRecording(mode) {
    if (recordingMode) stopRecording();
    else startRecording(mode);
  }

  async function processStt() {
    const blob = new Blob(audioChunksRef.current, { type: 'audio/webm' });
    onStatusChange?.('Transcribing...');
    try {
      const data = await apiChatTranscribe(blob);
      if (data.transcript?.trim()) setInputText(data.transcript);
      onStatusChange?.('Online');
    } catch { onStatusChange?.('Online'); }
  }

  async function processLive() {
    const blob = new Blob(audioChunksRef.current, { type: 'audio/webm' });
    setIsTyping(true);
    onStatusChange?.('Listening & Thinking...');
    try {
      const data = await apiChatAudio(blob, sessionId);
      setMessages(prev => [...prev,
        { type: 'user', text: `🎤 <i>${data.transcript}</i>` },
        { type: 'ai', text: data.response }
      ]);
      if (data.audio_base64) playAudioBase64(data.audio_base64);
      if (data.points_update) onScoreboardUpdate?.(data.new_points, data.new_level, data.new_kurals);
      onStatusChange?.('Online');
    } catch {
      setMessages(prev => [...prev, { type: 'system', text: '⚠️ Failed to send live message.' }]);
      onStatusChange?.('Online');
    } finally { setIsTyping(false); }
  }

  function handleGameComplete(gameData) {
    const msg = `I completed the '${gameData.title}' mini-game! I finished all ${gameData.steps.length} steps and earned the ${gameData.final_reward_badge} badge. Please analyze my performance and tell me how this helps my career!`;
    setInputText(msg);
  }

  if (!visible) return null;

  return (
    <div style={{ position: 'fixed', bottom: 24, right: 24, zIndex: 5000, display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 12 }}>
      {/* Chat Panel */}
      {isOpen && (
        <div className={`chat-panel-window${isFullscreen ? ' fullscreen' : ''}`} style={{
          width: isFullscreen ? '90vw' : 380,
          height: isFullscreen ? '85vh' : 520,
          background: 'var(--cream)',
          borderRadius: 20,
          boxShadow: '0 20px 60px rgba(0,0,0,0.15)',
          border: '1px solid var(--gray-200)',
          display: 'flex', flexDirection: 'column', overflow: 'hidden',
        }}>
          {/* Header */}
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '14px 16px', background: 'var(--black)', borderRadius: '20px 20px 0 0' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{ fontSize: 22 }}>👵</div>
              <div>
                <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--cream)' }}>Paati AI</div>
                <div style={{ fontSize: 11, color: 'var(--gray-400)' }}>
                  <span style={{ display: 'inline-block', width: 6, height: 6, borderRadius: '50%', background: 'var(--accent-teal)', marginRight: 4 }} />
                  {isTyping ? 'Thinking...' : 'Online'}
                </div>
              </div>
            </div>
            <div style={{ display: 'flex', gap: 8 }}>
              <button onClick={() => setIsFullscreen(f => !f)} style={{ background: 'none', border: 'none', color: 'var(--gray-400)', cursor: 'pointer', padding: 4 }}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{ width: 18, height: 18 }}><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" /></svg>
              </button>
              <button onClick={toggleOpen} style={{ background: 'none', border: 'none', color: 'var(--gray-400)', cursor: 'pointer', padding: 4 }}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{ width: 18, height: 18 }}><path d="M6 9l6 6 6-6" /></svg>
              </button>
            </div>
          </div>

          {/* Messages */}
          <div ref={messagesRef} style={{ flex: 1, overflowY: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }} className="scrollbar-hide">
            {messages.map((m, i) => (
              <ChatMessage key={i} msg={m} onGameComplete={handleGameComplete} />
            ))}
            {isTyping && (
              <div className="chat-msg chat-msg-ai">
                <div className="chat-bubble typing-indicator">
                  <span className="dot" /><span className="dot" /><span className="dot" />
                </div>
              </div>
            )}
          </div>

          {/* Input */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: 12, borderTop: '1px solid var(--gray-200)', background: 'white' }}>
            <button
              onClick={() => toggleRecording('stt')}
              className={recordingMode === 'stt' ? 'recording-pulse-stt' : ''}
              title="Voice to Text"
              style={{ width: 36, height: 36, borderRadius: 10, background: recordingMode === 'stt' ? '#ef4444' : 'var(--gray-100)', border: 'none', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: 'var(--black)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
                <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                <line x1="12" y1="19" x2="12" y2="22" />
              </svg>
            </button>
            <button
              onClick={() => toggleRecording('live')}
              className={recordingMode === 'live' ? 'recording-pulse-live' : ''}
              title="Live Voice Conversation"
              style={{ width: 36, height: 36, borderRadius: 10, background: recordingMode === 'live' ? '#8b5cf6' : 'var(--gray-100)', border: 'none', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: 'var(--black)' }}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
                <path d="M2 10v3" /><path d="M6 6v11" /><path d="M10 3v18" /><path d="M14 8v7" /><path d="M18 5v13" /><path d="M22 10v3" />
              </svg>
            </button>
            <input
              type="text"
              value={inputText}
              onChange={e => setInputText(e.target.value)}
              onKeyPress={e => e.key === 'Enter' && !e.shiftKey && sendMessage()}
              placeholder="Reply to Paati..."
              disabled={recordingMode !== null}
              style={{ flex: 1, padding: '8px 12px', background: 'var(--gray-100)', border: 'none', borderRadius: 10, fontSize: 14, color: 'var(--black)', outline: 'none', fontFamily: 'var(--font-primary)' }}
            />
            <button
              onClick={sendMessage}
              disabled={!inputText.trim() || !sessionId}
              style={{ width: 36, height: 36, borderRadius: 10, background: inputText.trim() ? 'var(--black)' : 'var(--gray-200)', border: 'none', cursor: inputText.trim() ? 'pointer' : 'default', display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: inputText.trim() ? 'var(--cream)' : 'var(--gray-400)' }}>
              <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16"><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" /></svg>
            </button>
          </div>
        </div>
      )}

      {/* FAB Toggle */}
      <button onClick={toggleOpen} style={{
        width: 56, height: 56, borderRadius: '50%', background: 'var(--black)', border: 'none',
        cursor: 'pointer', boxShadow: '0 8px 24px rgba(0,0,0,0.2)',
        display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 24,
        position: 'relative',
      }}>
        {isOpen ? (
          <svg viewBox="0 0 24 24" fill="currentColor" style={{ width: 22, height: 22, color: 'var(--cream)' }}>
            <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
          </svg>
        ) : '👵'}
        {!isOpen && (
          <span style={{ position: 'absolute', top: 4, right: 4, width: 10, height: 10, borderRadius: '50%', background: 'var(--accent-teal)', border: '2px solid var(--cream)', animation: 'pulse-anim 2s ease-in-out infinite' }} />
        )}
      </button>
    </div>
  );
}
