import React, { useState, useEffect } from 'react';
import { formatFeatureName } from '../../utils';

export function FactorCard({ factor, maxImpact, delay }) {
  const [animated, setAnimated] = useState(false);
  const isPositive = factor.impact > 0;
  const norm = (Math.abs(factor.impact) / maxImpact) * 100;

  useEffect(() => {
    const t = setTimeout(() => setAnimated(true), delay);
    return () => clearTimeout(t);
  }, [delay]);

  return (
    <div className={`factor-card ${animated ? 'animate' : ''}`}>
      <div className="factor-header">
        <span className="factor-name">{formatFeatureName(factor.feature)}</span>
        <span className={`factor-direction ${isPositive ? 'increases' : 'reduces'}`}>{factor.direction}</span>
      </div>
      <p className="factor-interpretation">{factor.interpretation}</p>
      <div className="factor-bar">
        <div className={`factor-bar-fill ${isPositive ? 'positive' : 'negative'}`}
          style={{ width: animated ? `${norm}%` : '0%', transition: 'width 1s ease' }} />
      </div>
    </div>
  );
}
