import React, { useState, useEffect, useRef } from 'react';

export function RiskCard({ prediction }) {
  const [displayPct, setDisplayPct] = useState('0.0');
  const ringRef = useRef(null);
  const pointerRef = useRef(null);

  useEffect(() => {
    if (!prediction) return;
    const pct = prediction.probability_percentage;

    let isMounted = true;
    let rafId;
    let startTimestamp = null;
    const duration = 1500;
    const step = (timestamp) => {
      if (!isMounted) return;
      if (!startTimestamp) startTimestamp = timestamp;
      const progress = Math.min((timestamp - startTimestamp) / duration, 1);
      setDisplayPct((progress * pct).toFixed(1));
      if (progress < 1) {
        rafId = window.requestAnimationFrame(step);
      }
    };
    rafId = window.requestAnimationFrame(step);

    const circ = 2 * Math.PI * 54;
    const offset = circ - (pct / 100) * circ;
    const timer = setTimeout(() => {
      if (isMounted) {
        if (ringRef.current) ringRef.current.style.strokeDashoffset = offset;
        if (pointerRef.current) pointerRef.current.style.left = `${pct}%`;
      }
    }, 100);

    return () => {
      isMounted = false;
      cancelAnimationFrame(rafId);
      clearTimeout(timer);
    };
  }, [prediction]);

  if (!prediction) return null;

  return (
    <div className={`risk-card ${prediction.risk_level?.toLowerCase()}`} style={{ marginBottom: 24 }}>
      <div className="risk-header">
        <span className="risk-label">Placement Chance</span>
        <span className="risk-confidence">{prediction.confidence}</span>
      </div>
      <div className="risk-display">
        <div className="risk-circle">
          <svg className="progress-ring" viewBox="0 0 120 120">
            <circle className="progress-ring-bg" cx="60" cy="60" r="54" />
            <circle ref={ringRef} className="progress-ring-fill" cx="60" cy="60" r="54"
              style={{ strokeDasharray: 339.29, strokeDashoffset: 339.29, transition: 'stroke-dashoffset 1.5s ease' }} />
          </svg>
          <div className="risk-value">
            <span className="risk-percentage">{displayPct}</span>
            <span className="risk-symbol">%</span>
          </div>
        </div>
        <div className="risk-level">{prediction.risk_level}</div>
      </div>
      <div className="risk-meter">
        <div className="meter-track">
          <div className="meter-fill" />
          <div ref={pointerRef} className="meter-pointer" style={{ left: '0%', transition: 'left 1s ease' }}>
            <div className="pointer-line" />
          </div>
        </div>
        <div className="meter-labels"><span>Low</span><span>Medium</span><span>High</span></div>
      </div>
    </div>
  );
}
