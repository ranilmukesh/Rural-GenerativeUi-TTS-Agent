import { useState } from 'react';
import './index.css';
import './chat.css';

import SidebarLeft from './components/SidebarLeft.jsx';
import SidebarRight from './components/SidebarRight.jsx';
import AssessmentForm from './components/AssessmentForm.jsx';
import ResultsPanel from './components/ResultsPanel.jsx';
import ChatView from './components/ChatView.jsx';

import { apiPredict, apiExplain, apiWhatIf } from './api.js';

/* ── Loading Overlay ─────────────────────────────────────────── */
function LoadingOverlay({ active }) {
  if (!active) return null;
  return (
    <div style={{
      position: 'fixed', inset: 0, background: '#2C1E16',
      display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
      zIndex: 9999,
    }}>
      <style>{`
        @keyframes pulse-ring {
          0%   { transform: scale(0.5); opacity: 1; }
          100% { transform: scale(1.5); opacity: 0; }
        }
        @keyframes heartbeat {
          0%,100% { transform: scale(1);    }
          50%      { transform: scale(1.15); }
        }
      `}</style>
      <div style={{ position: 'relative', width: 100, height: 100, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        {[0, 0.3, 0.6].map((d, i) => (
          <div key={i} style={{
            position: 'absolute', width: '100%', height: '100%',
            border: '2px solid #D35400', borderRadius: '50%',
            animation: `pulse-ring 1.5s ease-out ${d}s infinite`,
          }} />
        ))}
        <svg style={{ width: 40, height: 40, color: '#D35400', animation: 'heartbeat 1s ease-in-out infinite' }} viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z" />
        </svg>
      </div>
      <p style={{ color: '#FFF8E7', fontSize: 13, marginTop: 24, letterSpacing: '0.12em', textTransform: 'uppercase' }}>
        Paati is Analyzing...
      </p>
    </div>
  );
}

/* ── Form / Results wrapper (keeps the header + scroll shell) ── */
function MainContent({ view, prediction, explanation, whatif, formData, isLoading, onFormSubmit, onBack, onOpenChat, scoreboard }) {
  return (
    <main style={{
      flex: 1, display: 'flex', flexDirection: 'column',
      height: '100%', overflow: 'hidden', position: 'relative', background: '#f8f9fc',
    }}>
      {/* Top Header */}
      <header style={{ padding: '32px 40px 20px', display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', flexShrink: 0 }}>
        <div>
          <h1 style={{ fontFamily: 'Space Grotesk, sans-serif', fontSize: '1.8rem', fontWeight: 800, color: '#1e293b', display: 'flex', alignItems: 'center', gap: 10 }}>
            {view === 'form'
              ? <> Paati-Kural <span className="animate-wave" style={{ fontSize: '1.5rem', display: 'inline-block' }}>👋</span></>
              : <> Your Results <span style={{ fontSize: '1.5rem' }}>📊</span></>}
          </h1>
          <p style={{ color: '#64748b', marginTop: 6, fontSize: '1rem' }}>
            {view === 'form' ? 'Padichu, Velai Paaru — Study, Get a Job' : 'Powered by XGBoost + SHAP + Paati AI'}
          </p>
        </div>
        {/* Stat pills */}
        <div style={{ display: 'flex', gap: 10, flexShrink: 0 }}>
          {[{ v: '98.9%', l: 'Accuracy' }, { v: 'SHAP', l: 'Explainability' }, { v: '22', l: 'Features' }].map((s, i) => (
            <div key={i} style={{ background: '#2C1E16', color: '#FFF8E7', padding: '8px 14px', borderRadius: 14, textAlign: 'center', minWidth: 70 }}>
              <span style={{ display: 'block', fontFamily: 'Space Grotesk, sans-serif', fontSize: '1.1rem', fontWeight: 700, color: '#D35400' }}>{s.v}</span>
              <span style={{ display: 'block', fontSize: 10, color: '#a8a6a0', textTransform: 'uppercase', letterSpacing: '0.1em', marginTop: 2 }}>{s.l}</span>
            </div>
          ))}
        </div>
      </header>

      {/* Scrollable body */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '0 40px 100px' }} className="scrollbar-hide">

        {view === 'form' && (
          <>
            {/* Orb mascot */}
            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 24 }}>
              <div style={{ position: 'relative' }}>
                <div style={{
                  width: 72, height: 72, borderRadius: '50%',
                  background: 'linear-gradient(135deg,#ffb4c2,#f84d85,#5b4eff)',
                  boxShadow: '0 15px 40px -5px rgba(248,77,133,0.5)',
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 36, position: 'relative', overflow: 'hidden',
                }}>
                  <div style={{ position: 'absolute', top: 4, left: 8, width: 48, height: 28, background: 'rgba(255,255,255,0.25)', borderRadius: '50%', filter: 'blur(2px)', transform: 'rotate(-12deg)' }} />
                  👵
                </div>
                <div style={{
                  position: 'absolute', top: -8, right: -148,
                  background: 'white', border: '1px solid #f1f5f9',
                  boxShadow: '0 4px 12px rgba(0,0,0,0.06)', borderRadius: '16px 16px 16px 4px',
                  padding: '8px 14px', whiteSpace: 'nowrap',
                }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: '#475569' }}>Let's find your career path! 🎯</span>
                </div>
              </div>
            </div>
            <AssessmentForm onSubmit={onFormSubmit} isLoading={isLoading} />
          </>
        )}

        {view === 'results' && (
          <>
            {/* "Chat with Paati" CTA */}
            <div style={{ marginBottom: 20 }}>
              <button onClick={onOpenChat} style={{
                display: 'inline-flex', alignItems: 'center', gap: 10,
                padding: '12px 24px', background: 'linear-gradient(135deg,#5b4eff,#f84d85)',
                color: 'white', border: 'none', borderRadius: 14, fontWeight: 700,
                fontSize: 14, cursor: 'pointer', boxShadow: '0 4px 16px rgba(91,78,255,0.35)',
                transition: 'transform 0.2s',
              }}>
                <span style={{ fontSize: 18 }}>👵</span>
                Talk to Paati AI
                <span style={{ background: 'rgba(255,255,255,0.2)', padding: '2px 8px', borderRadius: 8, fontSize: 11 }}>New</span>
              </button>
            </div>
            <ResultsPanel
              prediction={prediction}
              explanation={explanation}
              whatif={whatif}
              formData={formData}
              onBack={onBack}
              scoreboard={scoreboard}
            />
          </>
        )}
      </div>

      {/* Bottom disclaimer */}
      <div style={{
        position: 'absolute', bottom: 0, left: 0, right: 0, pointerEvents: 'none',
        background: 'linear-gradient(to top,#f8f9fc 80%,transparent)',
        padding: '16px 40px 12px', textAlign: 'center', zIndex: 10,
      }}>
        <p style={{ fontSize: 11, color: '#94a3b8', fontWeight: 500, letterSpacing: '0.04em' }}>
          Paati-Kural League · AI-powered predictions · Verify important decisions.
        </p>
      </div>
    </main>
  );
}

/* ── Root App ─────────────────────────────────────────────────── */
export default function App() {
  const [view, setView]           = useState('form');   // 'form' | 'results' | 'chat'
  const [isLoading, setIsLoading] = useState(false);
  const [prediction, setPrediction]   = useState(null);
  const [explanation, setExplanation] = useState(null);
  const [whatif, setWhatif]           = useState(null);
  const [formData, setFormData]       = useState(null);
  const [chatStatus, setChatStatus]   = useState('Online');
  const [isThinking, setIsThinking]   = useState(false);
  const [scoreboard, setScoreboard]   = useState(null);
  // Chat state lifted here so it survives view switches
  const [chatMessages, setChatMessages]       = useState([]);
  const [chatSessionId, setChatSessionId]     = useState(null);
  const [chatInitialized, setChatInitialized] = useState(false);
  const [chatIsTyping, setChatIsTyping]       = useState(false);

  async function handleFormSubmit(data) {
    setIsLoading(true);
    setIsThinking(true);
    setFormData(data);
    setWhatif(null);
    try {
      const [pred, expl] = await Promise.all([apiPredict(data), apiExplain(data)]);
      setPrediction(pred);
      setExplanation(expl);
      setTimeout(() => {
        setIsLoading(false);
        setIsThinking(false);
        setView('results');
        // kick off what-if in background
        apiWhatIf(data).then(setWhatif).catch(() => {});
      }, 1000);
    } catch (err) {
      console.error(err);
      setIsLoading(false);
      setIsThinking(false);
      alert('Failed to get prediction. Please check the server is running.');
    }
  }

  function handleBack() {
    setView('form');
    setPrediction(null); setExplanation(null);
    setWhatif(null); setFormData(null);
    setScoreboard(null);
    setChatStatus('Online');
    // Reset chat so next analysis gets a fresh session
    setChatMessages([]);
    setChatSessionId(null);
    setChatInitialized(false);
    setChatIsTyping(false);
  }

  function handleScoreboardUpdate(points, level, kurals) {
    setScoreboard({ points, level, kurals });
  }

  return (
    <div style={{
      display: 'flex', height: '100vh', width: '100%',
      background: '#f8f9fc', fontFamily: 'Inter, sans-serif', overflow: 'hidden',
    }}>
      <LoadingOverlay active={isLoading} />

      <SidebarLeft />

      {/* Main area — switches between form/results and full chat */}
      {view === 'chat' ? (
        <ChatView
          studentData={formData}
          prediction={prediction}
          explanation={explanation}
          whatif={whatif}
          onScoreboardUpdate={handleScoreboardUpdate}
          onStatusChange={setChatStatus}
          onBack={() => setView('results')}
          // Lifted state — persists when switching views
          messages={chatMessages}
          setMessages={setChatMessages}
          sessionId={chatSessionId}
          setSessionId={setChatSessionId}
          initialized={chatInitialized}
          setInitialized={setChatInitialized}
          isTyping={chatIsTyping}
          setIsTyping={setChatIsTyping}
        />
      ) : (
        <MainContent
          view={view}
          prediction={prediction}
          explanation={explanation}
          whatif={whatif}
          formData={formData}
          isLoading={isLoading}
          onFormSubmit={handleFormSubmit}
          onBack={handleBack}
          onOpenChat={() => setView('chat')}
          scoreboard={scoreboard}
        />
      )}

      <SidebarRight
        chatActive={view === 'chat'}
        isThinking={isThinking}
        chatStatus={chatStatus}
        scoreboard={scoreboard}
      />
    </div>
  );
}
