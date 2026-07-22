// ConsensusRing — SVG progress ring for PoMV verification

interface ConsensusRingProps {
  /** PoMV score (0.0–1.0) */
  pomv: number;
  /** Ring diameter in px */
  size?: number;
  /** Whether to show label inside */
  showLabel?: boolean;
}

export function ConsensusRing({ pomv, size = 48, showLabel = true }: ConsensusRingProps) {
  const pct = Math.min(1, Math.max(0, pomv));
  const radius = (size - 6) / 2;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - pct);
  const displayPct = Math.round(pct * 100);

  // Color: red < 30%, yellow < 60%, green >= 60%
  const color = pct >= 0.6 ? 'var(--ob-success)' : pct >= 0.3 ? 'var(--ob-warning)' : 'var(--ob-error)';

  return (
    <div style={{ position: 'relative', width: size, height: size, flexShrink: 0 }} title={`${displayPct}% consensus`}>
      <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
        {/* Background track */}
        <circle
          cx={size / 2} cy={size / 2} r={radius}
          stroke="var(--ob-glass-border)" strokeWidth={3}
          fill="none"
        />
        {/* Progress arc */}
        <circle
          cx={size / 2} cy={size / 2} r={radius}
          stroke={color} strokeWidth={3}
          fill="none"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          strokeLinecap="round"
          style={{ transition: 'stroke-dashoffset 0.5s ease' }}
        />
      </svg>
      {showLabel && (
        <div style={{
          position: 'absolute', inset: 0,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontSize: size < 40 ? '0.55rem' : '0.65rem',
          fontWeight: 700, color,
        }}>
          {displayPct}%
        </div>
      )}
    </div>
  );
}
