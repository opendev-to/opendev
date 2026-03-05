import React from 'react';

function cn(...classes: Array<string | false | null | undefined>) {
  return classes.filter(Boolean).join(' ');
}

interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  size?: 'sm' | 'md';
  variant?: 'subtle' | 'ghost';
}

export function IconButton({
  className,
  size = 'sm',
  variant = 'subtle',
  children,
  ...props
}: IconButtonProps) {
  const sizes = {
    sm: 'p-2',
    md: 'p-2.5',
  }[size];

  const variants = {
    subtle: 'text-gray-600 hover:text-gray-900 bg-white hover:bg-gray-100 border border-gray-200 rounded-md',
    ghost: 'text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded-md',
  }[variant];

  return (
    <button className={cn('transition-colors', sizes, variants, className)} {...props}>
      {children}
    </button>
  );
}
