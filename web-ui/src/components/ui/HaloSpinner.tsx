import React, { useEffect, useState } from 'react';
import { SPINNER_FRAMES } from '../../constants/spinner';

export const HaloSpinner = React.memo(function HaloSpinner() {
  const [brailleOffset, setBrailleOffset] = useState(0);

  // ⚡ Bolt Performance Optimization:
  // Encapsulated this high-frequency state update (100ms interval) into an isolated
  // memoized component. Previously, this state lived in large parent components
  // (LandingPage and WelcomeScreen), causing deep re-renders of the entire
  // component tree 10 times per second and leading to significant layout thrashing.
  useEffect(() => {
    const interval = setInterval(() => {
      setBrailleOffset(prev => (prev + 1) % SPINNER_FRAMES.length);
    }, 100);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="absolute animate-spin-slow" style={{ width: 360, height: 360 }}>
      {Array.from({ length: 24 }).map((_, i) => {
        const angle = (i / 24) * 360;
        const char = SPINNER_FRAMES[(i + brailleOffset) % SPINNER_FRAMES.length];
        return (
          <span
            key={i}
            className="absolute text-lg font-mono text-bg-300"
            style={{
              left: '50%',
              top: '50%',
              transform: `rotate(${angle}deg) translateX(180px) rotate(-${angle}deg)`,
            }}
          >
            {char}
          </span>
        );
      })}
    </div>
  );
});
