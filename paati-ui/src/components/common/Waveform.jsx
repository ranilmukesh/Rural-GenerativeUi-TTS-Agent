import React from 'react';

export function Waveform({ bars = 20, active = false, colorClass = '#818cf8', flex = false }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 2, height: flex ? '100%' : 40, width: flex ? '100%' : 'auto' }}>
      {Array.from({ length: bars }).map((_, i) => {
        const h = active
          ? Math.random() * 80 + 20
          : 20 + Math.sin(i * 0.8) * 20 + Math.abs(Math.sin(i * 0.4)) * 40;
        return (
          <div key={i} style={{
            flex: flex ? 1 : 'none',
            width: flex ? 'auto' : 2, height: `${h}%`, borderRadius: 2,
            background: colorClass, opacity: flex ? 1 : 0.6,
            transition: active ? 'height 0.15s ease' : 'none',
          }} />
        );
      })}
    </div>
  );
}
