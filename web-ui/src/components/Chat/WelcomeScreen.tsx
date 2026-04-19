import { useEffect, useState } from 'react';
import { SPINNER_FRAMES } from '../../constants/spinner';

export function WelcomeScreen() {
  const [brailleOffset, setBrailleOffset] = useState(0);

  // Animate braille halo when welcome screen is visible
  useEffect(() => {
    const interval = setInterval(() => {
      setBrailleOffset(prev => (prev + 1) % SPINNER_FRAMES.length);
    }, 100);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="relative flex items-center justify-center h-full px-6 bg-bg-100 overflow-hidden">
      {/* Background watermark layer */}
      <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
        {/* "OpenDev" breathing text */}
        <span className="text-5xl md:text-7xl font-mono font-bold tracking-wider text-bg-300 animate-breathe select-none">
          OpenDev
        </span>
        {/* Orbiting braille halo ring */}
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
      </div>
      {/* Foreground welcome content */}
      <div className="relative z-10 text-center">
        <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-bg-200 flex items-center justify-center">
          <svg className="w-8 h-8 text-text-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
          </svg>
        </div>
        <h2 className="text-xl font-semibold text-text-000 mb-2">Welcome to OpenDev</h2>
        <p className="text-sm text-text-300">Start a conversation with your AI coding assistant</p>
      </div>
    </div>
  );
}
