import React, { useState } from 'react';
import { ThumbsUp, ThumbsDown, Copy } from 'lucide-react';
import { formatChatMarkdown } from '../../utils';
import { MiniGame } from './MiniGame';

// ── Tenali Puzzle Card ────────────────────────────────────────────────────────
// Rendered when the LLM outputs a ```json {"puzzle": "..."} ``` block.
function PuzzleCard({ puzzle }) {
  const [answer, setAnswer] = useState('');
  const [submitted, setSubmitted] = useState(false);

  return (
    <div style={{
      background: 'linear-gradient(135deg, #fef9ec, #fff8e1)',
      border: '2px solid #f59e0b',
      borderRadius: 16,
      padding: '18px 20px',
      marginTop: 8,
      width: '100%',
      boxShadow: '0 4px 20px rgba(245,158,11,0.12)',
    }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <span style={{ fontSize: 22 }}>🧩</span>
        <span style={{ fontSize: 12, fontWeight: 700, color: '#b45309', textTransform: 'uppercase', letterSpacing: '0.08em' }}>
          Paati Puzzle
        </span>
        <span style={{ marginLeft: 'auto', fontSize: 11, background: '#fde68a', color: '#92400e', padding: '2px 8px', borderRadius: 99, fontWeight: 600 }}>
          +50 Paati Points on correct answer!
        </span>
      </div>

      {/* Question */}
      <p style={{ fontSize: 14, color: '#1c1917', fontWeight: 500, lineHeight: 1.6, marginBottom: 14 }}>
        {puzzle}
      </p>

      {/* Input */}
      {!submitted ? (
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            type="text"
            placeholder="Type your answer here..."
            value={answer}
            onChange={e => setAnswer(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter' && answer.trim()) setSubmitted(true); }}
            style={{
              flex: 1, padding: '9px 14px', borderRadius: 10, border: '1.5px solid #fcd34d',
              fontSize: 13, outline: 'none', background: 'white', color: '#1c1917',
            }}
          />
          <button
            onClick={() => { if (answer.trim()) setSubmitted(true); }}
            style={{
              padding: '9px 18px', background: '#f59e0b', color: 'white',
              border: 'none', borderRadius: 10, fontWeight: 700, cursor: 'pointer', fontSize: 13,
            }}
          >
            Submit
          </button>
        </div>
      ) : (
        <div style={{
          padding: '10px 14px', background: '#d1fae5', borderRadius: 10,
          fontSize: 13, color: '#065f46', fontWeight: 600,
        }}>
          ✅ Answer submitted: "{answer}" — Paati will check it!
        </div>
      )}
    </div>
  );
}

// ── Extract all JSON blocks from message text ─────────────────────────────────
function extractJsonBlocks(text) {
  const blocks = [];
  const re = /```json\s*([\s\S]*?)\s*```|```\s*(\{[\s\S]*?\})\s*```/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const raw = m[1] || m[2];
    if (!raw) continue;
    try {
      blocks.push(JSON.parse(raw));
    } catch { /* skip unparseable blocks */ }
  }
  return blocks;
}

// ── Main Message component ────────────────────────────────────────────────────
export function Message({ msg, onGameComplete }) {
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

  // ── AI message ──────────────────────────────────────────────────────────────
  const jsonBlocks = extractJsonBlocks(msg.text || '');

  // Check for mini-game (has steps + title)
  const miniGame = jsonBlocks.find(b => b.steps && b.title);
  // Check for Tenali puzzle (has puzzle field)
  const puzzle = jsonBlocks.find(b => b.puzzle && !b.steps);

  // Strip code blocks from the visible text (formatChatMarkdown also strips them,
  // but we remove the whole fence here so there's no empty line residue)
  const textOnly = (msg.text || '')
    .replace(/```json[\s\S]*?```/g, '')
    .replace(/```[\s\S]*?```/g, '')
    .trim();

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-start', maxWidth: 580 }}>
      {/* Text bubble — only when there's text outside the code block */}
      {textOnly && (
        <div style={{
          background: 'white', border: '1px solid #f1f5f9',
          padding: '12px 16px', borderRadius: '18px 18px 18px 4px',
          fontSize: 14, color: '#334155', lineHeight: 1.65,
          boxShadow: '0 2px 12px rgba(0,0,0,0.04)',
          width: '100%',
        }} dangerouslySetInnerHTML={{ __html: formatChatMarkdown(msg.text) }} />
      )}

      {/* Tenali Puzzle card */}
      {puzzle && !miniGame && (
        <div style={{ width: '100%' }}>
          <PuzzleCard puzzle={puzzle.puzzle} />
        </div>
      )}

      {/* Full interactive mini-game */}
      {miniGame && (
        <div style={{ width: '100%' }}>
          <MiniGame gameData={miniGame} onComplete={onGameComplete} />
        </div>
      )}

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
