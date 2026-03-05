import type { Config } from 'tailwindcss'

export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        // Claude-style semantic tokens
        bg: {
          '000': 'hsl(var(--bg-000) / <alpha-value>)',
          '100': 'hsl(var(--bg-100) / <alpha-value>)',
          '200': 'hsl(var(--bg-200) / <alpha-value>)',
          '300': 'hsl(var(--bg-300) / <alpha-value>)',
          '400': 'hsl(var(--bg-400) / <alpha-value>)',
          '500': 'hsl(var(--bg-500) / <alpha-value>)',
        },
        text: {
          '000': 'hsl(var(--text-000) / <alpha-value>)',
          '100': 'hsl(var(--text-100) / <alpha-value>)',
          '200': 'hsl(var(--text-200) / <alpha-value>)',
          '300': 'hsl(var(--text-300) / <alpha-value>)',
          '400': 'hsl(var(--text-400) / <alpha-value>)',
          '500': 'hsl(var(--text-500) / <alpha-value>)',
        },
        border: {
          '100': 'hsl(var(--border-100) / <alpha-value>)',
          '200': 'hsl(var(--border-200) / <alpha-value>)',
          '300': 'hsl(var(--border-300) / <alpha-value>)',
          '400': 'hsl(var(--border-400) / <alpha-value>)',
        },
        accent: {
          main: {
            '100': 'hsl(var(--accent-main-100) / <alpha-value>)',
            '200': 'hsl(var(--accent-main-200) / <alpha-value>)',
          },
          secondary: {
            '100': 'hsl(var(--accent-secondary-100) / <alpha-value>)',
            '900': 'hsl(var(--accent-secondary-900) / <alpha-value>)',
          },
        },
        danger: {
          '000': 'hsl(var(--danger-000) / <alpha-value>)',
          '100': 'hsl(var(--danger-100) / <alpha-value>)',
        },
        success: {
          '000': 'hsl(var(--success-000) / <alpha-value>)',
          '100': 'hsl(var(--success-100) / <alpha-value>)',
        },
        warning: {
          '100': 'hsl(var(--warning-100) / <alpha-value>)',
        },
        // Keep gray palette for backward compatibility
        gray: {
          50: '#F9FAFB',
          100: '#F3F4F6',
          200: '#E5E7EB',
          300: '#D1D5DB',
          400: '#9CA3AF',
          500: '#6B7280',
          600: '#4B5563',
          700: '#374151',
          800: '#1F2937',
          900: '#111827',
        },
      },
      borderWidth: {
        '0.5': '0.5px',
      },
      fontFamily: {
        sans: ['-apple-system', 'BlinkMacSystemFont', 'Inter', 'Segoe UI', 'Helvetica Neue', 'Arial', 'sans-serif'],
        mono: ['JetBrains Mono', 'Fira Code', 'Monaco', 'Courier New', 'monospace'],
      },
      spacing: {
        '18': '4.5rem',
        '88': '22rem',
      },
      maxWidth: {
        '4.5xl': '58rem',
      },
      animation: {
        'slide-up': 'slide-up 0.3s ease-out',
        'fade-in': 'fade-in 0.2s ease-out',
        'breathe': 'breathe 4s ease-in-out infinite',
        'spin-slow': 'spin-slow 40s linear infinite',
      },
      keyframes: {
        'slide-up': {
          '0%': { opacity: '0', transform: 'translateY(10px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'breathe': {
          '0%, 100%': { opacity: '0.04' },
          '50%': { opacity: '0.10' },
        },
        'spin-slow': {
          '0%': { transform: 'rotate(0deg)' },
          '100%': { transform: 'rotate(360deg)' },
        },
      },
    },
  },
  plugins: [],
} satisfies Config
