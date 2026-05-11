import { Wrench, Sparkles, CheckCircle2, Circle, Check, ChevronDown, LayoutTemplate, FileText, MessageSquare } from 'lucide-react';
import { Waveform } from './common';

// Animated waveform bars

const thinkingSteps = [
  { label: 'Understanding the question', status: 'done' },
  { label: 'Analyzing student profile', status: 'active' },
  { label: 'Generating insights', status: 'pending' },
  { label: 'Preparing feedback', status: 'pending' },
];

const tools = [
  { title: 'Placement Predictor', desc: 'XGBoost model analyzed', icon: FileText, color: '#4f46e5', bg: '#ede9fe' },
  { title: 'SHAP Explainer', desc: 'Factor analysis done', icon: CheckCircle2, color: '#16a34a', bg: '#dcfce7' },
  { title: 'Paati AI', desc: 'Career advisor ready', icon: MessageSquare, color: '#0284c7', bg: '#e0f2fe' },
];

export default function SidebarRight({ chatActive = false, isThinking = false, chatStatus = 'Online', scoreboard = null }) {
  return (
    <aside style={{
      width: 300, height: '100%', background: 'rgba(255,255,255,0.5)',
      backdropFilter: 'blur(8px)', borderLeft: '1px solid rgba(226,232,240,0.6)',
      padding: 20, display: 'flex', flexDirection: 'column', gap: 24, overflowY: 'auto',
    }} className="scrollbar-hide">

      {/* Paati Scoreboard (shows after analysis) */}
      {scoreboard && (
        <div>
          <h3 style={{ fontSize: 13, fontWeight: 700, color: '#1e293b', marginBottom: 12 }}>Paati League</h3>
          <div className="paati-scoreboard">
            <div className="score-item" style={{ border: '2px solid #F39C12' }}>
              <span>Points</span>
              <strong style={{ color: '#F39C12' }}>{scoreboard.points}</strong>
            </div>
            <div className="score-item">
              <span>Level</span>
              <strong style={{ color: '#FFF8E7', fontSize: '0.8rem' }}>{scoreboard.level}</strong>
            </div>
            <div className="score-item">
              <span>Kurals</span>
              <strong style={{ color: '#27AE60' }}>{scoreboard.kurals}</strong>
            </div>
          </div>
        </div>
      )}

      {/* Voice Assistant Module */}
      <div>
        <h3 style={{ fontSize: 13, fontWeight: 700, color: '#1e293b', marginBottom: 12 }}>Voice Assistant</h3>
        <div style={{ marginBottom: 8 }}>
          {chatActive ? (
            <span style={{ fontSize: 12, color: '#4f46e5', fontWeight: 600 }} className="animate-pulse">
              🎙️ {chatStatus}
            </span>
          ) : (
            <span style={{ fontSize: 12, color: '#94a3b8', fontWeight: 500 }}>Idle — open chat to activate</span>
          )}
        </div>
        <div style={{ height: 40, marginBottom: 6 }}>
          <Waveform bars={35} active={chatActive} />
        </div>
        <div style={{ fontSize: 12, color: '#64748b', fontWeight: 500 }}>
          {chatActive ? 'Tamil / English' : '—'}
        </div>
      </div>

      {/* Thinking Module */}
      {isThinking && (
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <div style={{ width: 24, height: 24, borderRadius: '50%', background: '#ede9fe', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <Sparkles size={12} color="#6366f1" />
            </div>
            <h3 style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>Thinking</h3>
          </div>
          <p style={{ fontSize: 12, color: '#64748b', lineHeight: 1.6, marginBottom: 12 }}>
            Analyzing your placement profile...
          </p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 0, position: 'relative' }}>
            <div style={{ position: 'absolute', left: 10, top: 12, bottom: 16, width: 2, background: '#f1f5f9', zIndex: 0 }} />
            {thinkingSteps.map((item, i) => (
              <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 0', zIndex: 1 }}>
                <div style={{ width: 20, height: 20, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'white' }}>
                  {item.status === 'done' && <CheckCircle2 size={16} color="#22c55e" />}
                  {item.status === 'active' && (
                    <div style={{ position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                      <Circle size={16} color="#6366f1" fill="#ede9fe" />
                      <div style={{ position: 'absolute', width: 6, height: 6, background: '#6366f1', borderRadius: '50' }} className="animate-ping" />
                    </div>
                  )}
                  {item.status === 'pending' && <Circle size={14} color="#e2e8f0" strokeWidth={3} />}
                </div>
                <span style={{
                  fontSize: 13,
                  color: item.status === 'active' ? '#1e293b' : item.status === 'done' ? '#475569' : '#94a3b8',
                  fontWeight: item.status === 'active' ? 600 : 400,
                }}>
                  {item.label}
                </span>
                {item.status === 'done' && <Check size={13} color="#cbd5e1" style={{ marginLeft: 'auto' }} />}
                {item.status === 'active' && (
                  <div style={{ marginLeft: 'auto', display: 'flex', gap: 3 }}>
                    {[0, 150, 300].map(d => (
                      <div key={d} style={{ width: 4, height: 4, background: '#818cf8', borderRadius: '50%', animation: `bounce 1.2s ease-in-out ${d}ms infinite` }} />
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Tools & Actions */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
          <Wrench size={15} color="#94a3b8" />
          <h3 style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>Tools & Actions</h3>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {tools.map((tool, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'flex-start', gap: 12, padding: 12,
              borderRadius: 12, border: '1px solid #f1f5f9', background: 'white',
              boxShadow: '0 2px 8px -4px rgba(0,0,0,0.05)',
            }}>
              <div style={{ padding: 8, borderRadius: 8, background: tool.bg, color: tool.color, marginTop: 2 }}>
                <tool.icon size={15} />
              </div>
              <div>
                <div style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>{tool.title}</div>
                <div style={{ fontSize: 11, color: '#64748b', marginTop: 2 }}>{tool.desc}</div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Response / Status Module */}
      <div style={{ marginTop: 'auto' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
          <LayoutTemplate size={15} color="#94a3b8" />
          <h3 style={{ fontSize: 13, fontWeight: 700, color: '#1e293b' }}>Status</h3>
        </div>
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: 12, borderRadius: 12, border: '1px solid #f1f5f9', background: 'white', boxShadow: '0 1px 3px rgba(0,0,0,0.04)',
        }}>
          <span style={{ fontSize: 13, color: '#475569' }}>
            {chatActive ? chatStatus : 'Ready for analysis'}
          </span>
          <div style={{ width: 20, height: 20, borderRadius: '50%', background: chatActive ? '#22c55e' : '#94a3b8', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Check size={11} color="white" strokeWidth={3} />
          </div>
        </div>
      </div>

      {/* Model selector */}
      <div>
        <div style={{ fontSize: 10, color: '#94a3b8', textTransform: 'uppercase', letterSpacing: '0.1em', marginBottom: 6, padding: '0 4px' }}>Model</div>
        <button style={{
          width: '100%', display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '10px 12px', borderRadius: 10, border: '1px solid #e2e8f0', background: 'white',
          fontSize: 13, color: '#475569', cursor: 'pointer',
        }}>
          <span>Gemini + XGBoost</span>
          <ChevronDown size={13} color="#94a3b8" />
        </button>
      </div>
    </aside>
  );
}
