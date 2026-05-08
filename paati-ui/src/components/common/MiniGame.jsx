import React, { useState } from 'react';

export function MiniGame({ gameData, onComplete }) {
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
