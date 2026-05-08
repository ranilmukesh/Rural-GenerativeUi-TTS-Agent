import React from 'react';
import { ThumbsUp, ThumbsDown, Copy } from 'lucide-react';
import { formatChatMarkdown } from '../../utils';
import { MiniGame } from './MiniGame';

export function Message({ msg, onGameComplete }) {
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
